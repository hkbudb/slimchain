use async_trait::async_trait;
use slimchain_common::{
    basic::{BlockHeight, H256},
    create_id_type_u32,
    error::Result,
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
use slimchain_tx_state::{TxStateView, TxWriteSetPartialTrie};
use std::{sync::Arc, time::Duration};

create_id_type_u32!(TxEngineTaskId);

#[async_trait]
pub trait TxEngine {
    type Output: TxTrait;

    async fn execute_inner(&self, task: TxEngineTask) -> Result<(Self::Output, Duration)>;

    async fn execute(
        &self,
        task: TxEngineTask,
    ) -> Result<(Self::Output, TxWriteSetPartialTrie, Duration)> {
        let state_view = task.state_view.clone();
        let root_address = task.state_root;
        let (output, time) = self.execute_inner(task).await?;
        let write_trie = TxWriteSetPartialTrie::new(state_view, root_address, output.tx_writes())?;
        Ok((output, write_trie, time))
    }

    fn start(&self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub struct TxEngineTask {
    pub id: TxEngineTaskId,
    pub block_height: BlockHeight,
    pub state_view: Arc<dyn TxStateView + Sync + Send>,
    pub state_root: H256,
    pub signed_tx_req: SignedTxRequest,
}

impl TxEngineTask {
    pub fn new(
        block_height: BlockHeight,
        state_view: Arc<dyn TxStateView + Sync + Send>,
        state_root: H256,
        signed_tx_req: SignedTxRequest,
    ) -> Self {
        let id = TxEngineTaskId::next_id();

        Self {
            id,
            block_height,
            state_view,
            state_root,
            signed_tx_req,
        }
    }
}
