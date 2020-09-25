use crate::config::TEEConfig;
use dashmap::{mapref::one::RefMut as DashMapRefMut, DashMap};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use sgx_types::*;
use sgx_urts::SgxEnclave;
use slimchain_common::{
    basic::{BlockHeight, H256},
    error::{anyhow, Context as _, Error, Result},
    tx::SignedTx,
};
use slimchain_tee_sig::{AttestationReport, TEESignedTx};
use slimchain_tx_engine::{TxEngineWorker, TxTaskId};
use slimchain_tx_state::{TxStateReadContext, TxStateView};
use slimchain_utils::path::binary_directory;
use std::{
    mem,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) static TASK_STATES: Lazy<DashMap<TxTaskId, TaskState>> = Lazy::new(DashMap::new);
pub(crate) type SharedSgxEnclave = Arc<SgxEnclave>;

pub struct TEETxEngineWorkerFactory {
    enclave: SharedSgxEnclave,
    attest_pk: Arc<AttestTEEPublicKey>,
}

impl TEETxEngineWorkerFactory {
    pub fn new(config: TEEConfig, enclave_path: &Path) -> Result<Self> {
        info!("Init SGX enclave from {}.", enclave_path.display());
        let debug = 1;
        let mut launch_token: sgx_launch_token_t = unsafe { mem::zeroed() };
        let mut launch_token_updated: i32 = 0;
        let mut misc_attr: sgx_misc_attribute_t = unsafe { mem::zeroed() };
        let enclave = SgxEnclave::create(
            enclave_path,
            debug,
            &mut launch_token,
            &mut launch_token_updated,
            &mut misc_attr,
        )
        .map_err(Error::msg)?;
        let enclave = Arc::new(enclave);
        let attest_pk = AttestTEEPublicKey::new(config, enclave.clone())?;

        Ok(Self { enclave, attest_pk })
    }

    pub fn use_enclave_in_the_same_dir(config: TEEConfig) -> Result<Self> {
        let dir = binary_directory()?;
        Self::new(config, &dir.join(env!("ENCLAVE_FILE_NAME")))
    }

    pub fn use_test(config: TEEConfig) -> Result<Self> {
        Self::new(
            config,
            &PathBuf::from(env!("ENCLAVE_FILE_DIR")).join(env!("ENCLAVE_FILE_NAME")),
        )
    }

    pub fn worker(&self) -> Box<TEETxEngineWorker> {
        Box::new(TEETxEngineWorker::new(
            self.enclave.clone(),
            self.attest_pk.clone(),
        ))
    }
}

pub struct TEETxEngineWorker {
    enclave: SharedSgxEnclave,
    attest_pk: Arc<AttestTEEPublicKey>,
}

impl TEETxEngineWorker {
    fn new(enclave: SharedSgxEnclave, attest_pk: Arc<AttestTEEPublicKey>) -> Self {
        Self { enclave, attest_pk }
    }
}

impl TxEngineWorker for TEETxEngineWorker {
    type Output = TEESignedTx;

    fn execute(
        &self,
        id: TxTaskId,
        block_height: BlockHeight,
        state_view: Arc<dyn TxStateView + Sync + Send>,
        state_root: H256,
        signed_tx_req: SignedTxRequest,
    ) -> Result<Self::Output> {
        let task_state_guard = TaskStateGuard::new(id, state_root, state_view.clone());
        crate::ecall::exec_tx(&self.enclave, id, block_height, state_root, &signed_tx_req)?;
        let SignedTx { raw_tx, pk_sig } = TaskState::get_task_state(id)?.take_result()?;
        task_state_guard.finish();
        let attest_report = self.attest_pk.get_attest_report()?;
        Ok(TEESignedTx {
            raw_tx,
            pk_sig,
            attest_report,
        })
    }
}

pub(crate) struct TaskState {
    read_ctx: TxStateReadContext,
    read_proof: Option<Vec<u8>>,
    signed_tx: Option<SignedTx>,
}

impl TaskState {
    pub(crate) fn get_task_state(
        id: TxTaskId,
    ) -> Result<DashMapRefMut<'static, TxTaskId, TaskState>> {
        TASK_STATES
            .get_mut(&id)
            .ok_or_else(|| anyhow!("Task state cannot be found (id: {}).", id))
    }

    pub(crate) fn get_read_ctx_mut(&mut self) -> &mut TxStateReadContext {
        &mut self.read_ctx
    }

    pub(crate) fn get_read_proof(&mut self) -> Result<&Vec<u8>> {
        if self.read_proof.is_none() {
            let proof = self.read_ctx.generate_proof()?;
            self.read_proof = Some(postcard::to_allocvec(&proof)?);
        }

        Ok(self.read_proof.as_ref().unwrap())
    }

    pub(crate) fn set_result(&mut self, signed_tx: SignedTx) {
        self.signed_tx = Some(signed_tx);
    }

    pub(crate) fn take_result(&mut self) -> Result<SignedTx> {
        self.signed_tx
            .take()
            .context("Task result is not available.")
    }
}

struct TaskStateGuard {
    task_id: TxTaskId,
}

impl TaskStateGuard {
    fn new(
        task_id: TxTaskId,
        state_root: H256,
        state_view: Arc<dyn TxStateView + Sync + Send>,
    ) -> Self {
        let state = TaskState {
            read_ctx: TxStateReadContext::new(state_view, state_root),
            read_proof: None,
            signed_tx: None,
        };
        TASK_STATES.insert(task_id, state);
        Self { task_id }
    }

    fn finish(self) {}
}

impl Drop for TaskStateGuard {
    fn drop(&mut self) {
        TASK_STATES.remove(&self.task_id);
    }
}

const ATTEST_REPORT_TTL_SECS: u64 = 21_600; // 6 hours

struct AttestTEEPublicKey {
    config: TEEConfig,
    enclave: SharedSgxEnclave,
    attest_report: RwLock<(AttestationReport, Instant)>,
}

impl AttestTEEPublicKey {
    fn new(config: TEEConfig, enclave: SharedSgxEnclave) -> Result<Arc<Self>> {
        let attest_report = crate::ecall::quote_pk(&enclave, &config)?;
        Ok(Arc::new(Self {
            config,
            enclave,
            attest_report: RwLock::new((attest_report, Instant::now())),
        }))
    }

    fn get_attest_report(self: &Arc<Self>) -> Result<AttestationReport> {
        let read_lock = self.attest_report.read();
        let now = Instant::now();
        if now - read_lock.1 <= Duration::from_secs(ATTEST_REPORT_TTL_SECS) {
            return Ok(read_lock.0.clone());
        }
        mem::drop(read_lock);

        let mut write_lock = self.attest_report.write();
        let now = Instant::now();
        if now - write_lock.1 <= Duration::from_secs(ATTEST_REPORT_TTL_SECS) {
            return Ok(write_lock.0.clone());
        }
        let new_attest_report = crate::ecall::quote_pk(&self.enclave, &self.config)?;
        *write_lock = (new_attest_report.clone(), now);

        Ok(new_attest_report)
    }
}
