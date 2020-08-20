use slimchain_common::{
    basic::{Address, Code, Nonce, StateKey, StateValue, H160, H256},
    error::Result,
};
use slimchain_tx_engine::TxTaskId;
use std::{cmp::min, ptr::copy_nonoverlapping, slice};

#[inline]
unsafe fn get_address_from_raw(acc_address: *const u8) -> Address {
    let buf = slice::from_raw_parts(acc_address, 20);
    H160::from_slice(buf).into()
}

#[inline]
unsafe fn get_state_key_from_raw(acc_address: *const u8) -> StateKey {
    let buf = slice::from_raw_parts(acc_address, 32);
    H256::from_slice(buf).into()
}

macro_rules! try_run {
    ($x: expr) => {
        match $x {
            Ok(tmp) => tmp,
            Err(e) => {
                error!(
                    "TEETxEngine: ocall failed [{}:{}] {:?}.",
                    file!(),
                    line!(),
                    e
                );
                return 1;
            }
        }
    };
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_nonce(id: u32, acc_address: *const u8, nonce: *mut u8) -> i32 {
    let acc_address = get_address_from_raw(acc_address);
    let n = try_run!(get_nonce(id.into(), acc_address));
    let dst = slice::from_raw_parts_mut(nonce, 32);
    n.to_little_endian(dst);
    0
}

#[inline]
fn get_nonce(id: TxTaskId, acc_address: Address) -> Result<Nonce> {
    let mut task_state = crate::engine::TaskState::get_task_state(id)?;
    task_state.get_read_ctx_mut().get_nonce(acc_address)
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_code_len(
    id: u32,
    acc_address: *const u8,
    code_len: *mut usize,
) -> i32 {
    let acc_address = get_address_from_raw(acc_address);
    *code_len = try_run!(get_code_len(id.into(), acc_address));
    0
}

#[inline]
fn get_code_len(id: TxTaskId, acc_address: Address) -> Result<usize> {
    let mut task_state = crate::engine::TaskState::get_task_state(id)?;
    task_state.get_read_ctx_mut().get_code_len(acc_address)
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_code(
    id: u32,
    acc_address: *const u8,
    code: *mut u8,
    code_len: usize,
) -> i32 {
    let acc_address = get_address_from_raw(acc_address);
    let c = try_run!(get_code(id.into(), acc_address));
    let len = min(code_len, c.len());
    copy_nonoverlapping(c.as_ptr(), code, len);
    0
}

#[inline]
fn get_code(id: TxTaskId, acc_address: Address) -> Result<Code> {
    let mut task_state = crate::engine::TaskState::get_task_state(id)?;
    task_state.get_read_ctx_mut().get_code(acc_address)
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_value(
    id: u32,
    acc_address: *const u8,
    key: *const u8,
    value: *mut u8,
) -> i32 {
    let acc_address = get_address_from_raw(acc_address);
    let key = get_state_key_from_raw(key);
    let v = try_run!(get_value(id.into(), acc_address, key));
    copy_nonoverlapping(v.as_bytes().as_ptr(), value, 32);
    0
}

#[inline]
fn get_value(id: TxTaskId, acc_address: Address, key: StateKey) -> Result<StateValue> {
    let mut task_state = crate::engine::TaskState::get_task_state(id)?;
    task_state.get_read_ctx_mut().get_value(acc_address, key)
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_read_proof_len(id: u32, proof_len: *mut usize) -> i32 {
    *proof_len = try_run!(get_read_proof_len(id.into()));
    0
}

#[inline]
fn get_read_proof_len(id: TxTaskId) -> Result<usize> {
    let mut task_state = crate::engine::TaskState::get_task_state(id)?;
    task_state.get_read_proof().map(|p| p.len())
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_read_proof(id: u32, proof: *mut u8, proof_len: usize) -> i32 {
    let mut task_state = try_run!(crate::engine::TaskState::get_task_state(id.into()));
    let p = try_run!(task_state.get_read_proof());
    let len = min(proof_len, p.len());
    copy_nonoverlapping(p.as_ptr(), proof, len);
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_return_result(id: u32, result: *const u8, result_len: usize) -> i32 {
    let mut task_state = try_run!(crate::engine::TaskState::get_task_state(id.into()));
    let signed_tx = {
        let buf = slice::from_raw_parts(result, result_len);
        try_run!(postcard::from_bytes(&buf))
    };
    task_state.set_result(signed_tx);
    0
}
