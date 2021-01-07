use crate::behavior::raft::{
    client::ClientNodeRaft,
    client_storage::ClientNodeStorage,
    message::{NewBlockRequest, NewBlockResponse},
};
use async_raft::{
    error::ClientWriteError,
    raft::{ClientWriteRequest, ClientWriteResponse},
};
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use slimchain_chain::{
    behavior::propose_block,
    block_proposal::BlockProposal,
    config::{ChainConfig, MinerConfig},
    consensus::raft::{create_new_block, Block},
};
use slimchain_common::{
    error::{bail, Result},
    tx::TxTrait,
};
use slimchain_tx_state::TxProposal;
use slimchain_utils::record_event;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct BlockProposalWorker<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<TxProposal<Tx>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> BlockProposalWorker<Tx> {
    pub fn new(
        chain_cfg: &ChainConfig,
        miner_cfg: &MinerConfig,
        raft_storage: Arc<ClientNodeStorage<Tx>>,
        raft: Arc<ClientNodeRaft<Tx>>,
        mut block_proposal_broadcast_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<TxProposal<Tx>>();
        let mut tx_rx = tx_rx.fuse().peekable();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let chain_cfg = chain_cfg.clone();
        let miner_cfg = miner_cfg.clone();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    res = Pin::new(&mut tx_rx).peek() => {
                        if res.is_none() {
                            break;
                        }
                    }
                }

                let mut snapshot = raft_storage.latest_snapshot().await;
                let blk_proposal = match propose_block(
                    &chain_cfg,
                    &miner_cfg,
                    &mut snapshot,
                    &mut tx_rx,
                    create_new_block,
                )
                .await
                {
                    Ok(blk_proposal) => blk_proposal,
                    Err(e) => {
                        error!("Failed to build the new block. Error: {}", e);
                        continue;
                    }
                };

                let blk_proposal = match blk_proposal {
                    Some(blk_proposal) => blk_proposal,
                    None => break,
                };

                raft_storage
                    .set_miner_snapshot(&blk_proposal, snapshot)
                    .await;

                match raft
                    .client_write(ClientWriteRequest::new(NewBlockRequest(
                        blk_proposal.clone(),
                    )))
                    .await
                {
                    Ok(ClientWriteResponse { data, .. }) => match data {
                        NewBlockResponse::Ok => {}
                        NewBlockResponse::Err(e) => {
                            error!("Raft write error from response. Error: {}", e);

                            for tx in blk_proposal.get_txs() {
                                let tx_id = tx.id();
                                record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_response", "detail": std::format!("{}", e));
                            }

                            continue;
                        }
                    },
                    Err(ClientWriteError::ForwardToLeader(_, leader)) => {
                        warn!("Raft write should be forward to leader ({:?}).", leader);

                        for tx in blk_proposal.get_txs() {
                            let tx_id = tx.id();
                            record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_non_leader", "detail": std::format!("leader={:?}", leader));
                        }

                        continue;
                    }
                    Err(ClientWriteError::RaftError(e)) => {
                        error!("Raft write error from raft. Error: {}", e);

                        for tx in blk_proposal.get_txs() {
                            let tx_id = tx.id();
                            record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_error", "detail": std::format!("{}", e));
                        }

                        continue;
                    }
                }

                block_proposal_broadcast_tx.send(blk_proposal).await.ok();
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn get_tx_tx(&self) -> mpsc::UnboundedSender<TxProposal<Tx>> {
        self.tx_tx.clone()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.tx_tx.close_channel();
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            shutdown_tx.send(()).ok();
        } else {
            bail!("Already shutdown.");
        }
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
