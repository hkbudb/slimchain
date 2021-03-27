use sgx_tse::rsgx_create_report;
use sgx_types::*;
use slimchain_common::{
    ed25519::ed25519_dalek::PUBLIC_KEY_LENGTH,
    error::{anyhow, Result},
};
use std::prelude::v1::*;

#[no_mangle]
pub unsafe extern "C" fn ecall_quote_pk(
    quote_target: *const sgx_target_info_t,
    report: *mut sgx_report_t,
) -> i32 {
    *report = match quote_pk(*quote_target) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("[Enclave Error] Failed to generate quote report.");
            eprintln!(" DETAIL: {}", e);
            return 1;
        }
    };

    0
}

fn quote_pk(quote_target: sgx_target_info_t) -> Result<sgx_report_t> {
    let mut report_data = sgx_report_data_t::default();
    report_data.d[..PUBLIC_KEY_LENGTH]
        .copy_from_slice(&crate::get_key_pair().public.as_bytes()[..]);
    rsgx_create_report(&quote_target, &report_data)
        .map_err(|e| anyhow!("Failed to create report. Reason: {}.", e))
}
