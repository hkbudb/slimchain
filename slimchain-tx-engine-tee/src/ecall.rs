use crate::{
    config::TEEConfig,
    engine::SharedSgxEnclave,
    intel_api::{get_intel_report, get_intel_sigrl},
};
use rand::{thread_rng, Rng};
use sgx_types::*;
use slimchain_common::{
    basic::{BlockHeight, H256},
    error::{ensure, Result},
    tx_req::SignedTxRequest,
};
use slimchain_tee_sig::AttestationReport;
use slimchain_tx_engine::TxTaskId;
use std::ptr;

mod ffi {
    #![allow(clippy::all)]
    #![allow(dead_code)]
    use sgx_types::*;

    include!(concat!(env!("OUT_DIR"), "/enclave_ffi.rs"));
}

pub(crate) fn exec_tx(
    enclave: &SharedSgxEnclave,
    id: TxTaskId,
    block_height: BlockHeight,
    state_root: H256,
    signed_tx_req: &SignedTxRequest,
) -> Result<()> {
    let mut ret: i32 = 0;
    let tx_req_data = postcard::to_allocvec(signed_tx_req)?;
    let sgx_ret = unsafe {
        ffi::ecall_exec_tx(
            enclave.geteid(),
            &mut ret as *mut _,
            id.into(),
            block_height.into(),
            state_root.as_bytes().as_ptr(),
            tx_req_data.as_ptr(),
            tx_req_data.len(),
        )
    };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "TEETxEngine: SGX error {:?}.",
        sgx_ret
    );
    ensure!(ret == 0, "TEETxEngine: Failed to execute tx.");
    Ok(())
}

pub(crate) fn quote_pk(
    enclave: &SharedSgxEnclave,
    config: &TEEConfig,
) -> Result<AttestationReport> {
    // init quote
    let mut quote_target = sgx_target_info_t::default();
    let mut quote_gid = sgx_epid_group_id_t::default();
    let sgx_ret = unsafe { sgx_init_quote(&mut quote_target as *mut _, &mut quote_gid as *mut _) };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "TEETxEngine: Failed to init quote. SGX error {:?}.",
        sgx_ret
    );

    // get quote report
    let mut ret: i32 = 0;
    let mut report = sgx_report_t::default();
    let sgx_ret = unsafe {
        ffi::ecall_quote_pk(
            enclave.geteid(),
            &mut ret as *mut _,
            &quote_target as *const _,
            &mut report as *mut _,
        )
    };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "TEETxEngine: Failed to get quote report. SGX error {:?}.",
        sgx_ret
    );
    ensure!(ret == 0, "TEETxEngine: Failed to get quote report.");

    // calculate quote size
    let sigrl = get_intel_sigrl(quote_gid, &config.api_key)?;
    let mut quote_size: u32 = 0;
    let (p_sigrl, sigrl_len) = if sigrl.is_empty() {
        (ptr::null(), 0)
    } else {
        (sigrl.as_ptr(), sigrl.len() as u32)
    };
    let sgx_ret = unsafe { sgx_calc_quote_size(p_sigrl, sigrl_len, &mut quote_size as *mut _) };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "TEETxEngine: Failed to calculate quote size. SGX error {:?}.",
        sgx_ret
    );

    // get quote
    let quote_type = if config.linkable {
        sgx_quote_sign_type_t::SGX_LINKABLE_SIGNATURE
    } else {
        sgx_quote_sign_type_t::SGX_UNLINKABLE_SIGNATURE
    };
    let nonce = {
        let mut nonce = sgx_quote_nonce_t::default();
        let mut rng = thread_rng();
        rng.fill(&mut nonce.rand);
        nonce
    };
    let mut quote_buf = Vec::with_capacity(quote_size as usize);
    let mut _quote_report = sgx_report_t::default();
    let sgx_ret = unsafe {
        sgx_get_quote(
            &report,
            quote_type,
            config.spid.as_ptr() as *const _,
            &nonce,
            p_sigrl,
            sigrl_len,
            &mut _quote_report as *mut _,
            quote_buf.as_mut_ptr() as *mut _,
            quote_size,
        )
    };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "TEETxEngine: Failed to get quote. SGX error {:?}.",
        sgx_ret
    );
    unsafe {
        quote_buf.set_len(quote_size as usize);
    }

    get_intel_report(&quote_buf[..], &config.api_key)
}
