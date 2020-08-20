use crate::KEY_PAIR;
use sgx_types::*;
use slimchain_common::{
    basic::{Address, BlockHeight, Code, Nonce, StateKey, StateValue, H256, U256},
    error::{anyhow, ensure, Result},
    tx::{RawTx, SignedTx},
    tx_req::SignedTxRequest,
};
use slimchain_tx_state::TxReadProof;
use std::prelude::v1::*;
use std::{mem::MaybeUninit, slice};

extern "C" {
    fn ocall_get_nonce(
        retval: *mut i32,
        id: u32,
        acc_address: *const u8,
        nonce: *mut u8,
    ) -> sgx_status_t;
    fn ocall_get_code_len(
        retval: *mut i32,
        id: u32,
        acc_address: *const u8,
        code_len: *mut usize,
    ) -> sgx_status_t;
    fn ocall_get_code(
        retval: *mut i32,
        id: u32,
        acc_address: *const u8,
        code: *mut u8,
        code_len: usize,
    ) -> sgx_status_t;
    fn ocall_get_value(
        retval: *mut i32,
        id: u32,
        acc_address: *const u8,
        key: *const u8,
        value: *mut u8,
    ) -> sgx_status_t;
    fn ocall_get_read_proof_len(retval: *mut i32, id: u32, proof_len: *mut usize) -> sgx_status_t;
    fn ocall_get_read_proof(
        retval: *mut i32,
        id: u32,
        proof: *mut u8,
        proof_len: usize,
    ) -> sgx_status_t;
    fn ocall_return_result(
        retval: *mut i32,
        id: u32,
        result: *const u8,
        result_len: usize,
    ) -> sgx_status_t;
}

#[no_mangle]
pub unsafe extern "C" fn ecall_exec_tx(
    id: u32,
    block_height: u64,
    state_root: *const u8,
    signed_tx_req: *const u8,
    req_len: usize,
) -> i32 {
    let state_root = {
        let buf = slice::from_raw_parts(state_root, 32);
        H256::from_slice(buf)
    };
    let signed_tx_req: SignedTxRequest = {
        let buf = slice::from_raw_parts(signed_tx_req, req_len);
        match postcard::from_bytes(buf) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("[Enclave Error] Failed to deserialize signed_tx_req.");
                eprintln!(" DETAIL: {}", e);
                return 1;
            }
        }
    };
    let signed_tx = match exec_tx(id, block_height.into(), state_root, signed_tx_req) {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("[Enclave Error] Failed to execute tx.");
            eprintln!(" DETAIL: {}", e);
            return 1;
        }
    };
    let signed_tx_buf = match postcard::to_allocvec(&signed_tx) {
        Ok(buf) => buf,
        Err(e) => {
            eprintln!("[Enclave Error] Failed to serialize signed_tx.");
            eprintln!(" DETAIL: {}", e);
            return 1;
        }
    };
    let mut retval: i32 = 0;
    let sgx_ret = ocall_return_result(
        &mut retval as *mut _,
        id,
        signed_tx_buf.as_ptr(),
        signed_tx_buf.len(),
    );
    if sgx_ret != sgx_status_t::SGX_SUCCESS || retval != 0 {
        eprintln!("[Enclave Error] Failed to return signed_tx.");
        eprintln!(" DETAIL: sgx_ret={}, retval={}.", sgx_ret, retval);
        return 1;
    }
    0
}

struct Backend {
    id: u32,
}

impl slimchain_tx_executor::Backend for Backend {
    fn get_nonce(&self, acc_address: Address) -> Result<Nonce> {
        let mut retval: i32 = 0;
        let mut nonce = MaybeUninit::<[u8; 32]>::uninit();
        let sgx_ret = unsafe {
            ocall_get_nonce(
                &mut retval as *mut _,
                self.id,
                acc_address.as_bytes().as_ptr(),
                nonce.as_mut_ptr() as *mut _,
            )
        };
        ensure!(
            sgx_ret == sgx_status_t::SGX_SUCCESS,
            "Failed to get nonce (address: {}). Reason: {}.",
            acc_address,
            sgx_ret
        );
        ensure!(
            retval == 0,
            "Failed to get nonce (address: {}).",
            acc_address
        );
        let nonce = unsafe { nonce.assume_init() };
        Ok(U256::from_little_endian(&nonce[..]).into())
    }
    fn get_code(&self, acc_address: Address) -> Result<Code> {
        let mut retval: i32 = 0;
        let mut code_len: usize = 0;
        let sgx_ret = unsafe {
            ocall_get_code_len(
                &mut retval as *mut _,
                self.id,
                acc_address.as_bytes().as_ptr(),
                &mut code_len as *mut _,
            )
        };
        ensure!(
            sgx_ret == sgx_status_t::SGX_SUCCESS,
            "Failed to get code len (address: {}). Reason: {}.",
            acc_address,
            sgx_ret
        );
        ensure!(
            retval == 0,
            "Failed to get code len (address: {}).",
            acc_address
        );

        if code_len == 0 {
            return Ok(Code::default());
        }

        let mut code = Vec::with_capacity(code_len);
        let sgx_ret = unsafe {
            ocall_get_code(
                &mut retval as *mut _,
                self.id,
                acc_address.as_bytes().as_ptr(),
                code.as_mut_ptr() as *mut _,
                code_len,
            )
        };
        ensure!(
            sgx_ret == sgx_status_t::SGX_SUCCESS,
            "Failed to get code (address: {}). Reason: {}.",
            acc_address,
            sgx_ret
        );
        ensure!(retval == 0, "Failed to get code (address:{}).", acc_address);
        unsafe {
            code.set_len(code_len);
        }
        Ok(code.into())
    }
    fn get_value(&self, acc_address: Address, key: StateKey) -> Result<StateValue> {
        let mut retval: i32 = 0;
        let mut value = MaybeUninit::<[u8; 32]>::uninit();
        let sgx_ret = unsafe {
            ocall_get_value(
                &mut retval as *mut _,
                self.id,
                acc_address.as_bytes().as_ptr(),
                key.as_bytes().as_ptr(),
                value.as_mut_ptr() as *mut _,
            )
        };
        ensure!(
            sgx_ret == sgx_status_t::SGX_SUCCESS,
            "Failed to get value (address: {}, key: {}). Reason: {}.",
            acc_address,
            key,
            sgx_ret
        );
        ensure!(
            retval == 0,
            "Failed to get value (address: {}, key: {}).",
            acc_address,
            key
        );
        let value = unsafe { value.assume_init() };
        Ok(H256::from_slice(&value[..]).into())
    }
}

fn get_read_proof(id: u32) -> Result<TxReadProof> {
    let mut retval: i32 = 0;

    let mut proof_len: usize = 0;
    let sgx_ret =
        unsafe { ocall_get_read_proof_len(&mut retval as *mut _, id, &mut proof_len as *mut _) };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "Failed to get read proof len. Reason: {}.",
        sgx_ret
    );
    ensure!(retval == 0, "Failed to get read proof len.");

    let mut proof_buf = Vec::with_capacity(proof_len);
    let sgx_ret = unsafe {
        ocall_get_read_proof(
            &mut retval as *mut _,
            id,
            proof_buf.as_mut_ptr() as *mut _,
            proof_len,
        )
    };
    ensure!(
        sgx_ret == sgx_status_t::SGX_SUCCESS,
        "Failed to get read proof. Reason: {}.",
        sgx_ret
    );
    ensure!(retval == 0, "Failed to get read proof.");
    unsafe {
        proof_buf.set_len(proof_len);
    }
    postcard::from_bytes::<TxReadProof>(&proof_buf[..])
        .map_err(|e| anyhow!("Failed to deserialize read proof. Reason: {}.", e))
}

fn exec_tx(
    id: u32,
    block_height: BlockHeight,
    state_root: H256,
    signed_tx_req: SignedTxRequest,
) -> Result<SignedTx> {
    let backend = Backend { id };

    let exec_output = slimchain_tx_executor::execute_tx(signed_tx_req, &backend)?;
    let read_proof = get_read_proof(id)?;
    read_proof.verify(&exec_output.reads, state_root)?;

    let raw_tx = RawTx {
        caller: exec_output.caller,
        input: exec_output.input,
        block_height,
        state_root,
        reads: exec_output.reads.to_set(),
        writes: exec_output.writes,
    };

    let signed_tx = raw_tx.sign(&*KEY_PAIR);

    Ok(signed_tx)
}
