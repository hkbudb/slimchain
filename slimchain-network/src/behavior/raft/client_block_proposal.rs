use crate::behavior::raft::{
    client::ClientNodeRaft,
    client_storage::ClientNodeStorage,
    message::{NewBlockRequest, NewBlockResponse},
};
use async_raft::{
    error::ClientWriteError,
    raft::{ClientWriteRequest, ClientWriteResponse},
};
use futures::{channel::mpsc, prelude::*};
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
use std::{pin::Pin, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};

pub struct BlockProposalWorker<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<TxProposal<Tx>>,
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> BlockProposalWorker<Tx> {
    pub fn new(
        chain_cfg: &ChainConfig,
        miner_cfg: &MinerConfig,
        raft_storage: Arc<ClientNodeStorage<Tx>>,
        raft: Arc<ClientNodeRaft<Tx>>,
        raft_client_lock: Arc<Mutex<()>>,
        mut block_proposal_broadcast_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<TxProposal<Tx>>();
        let mut tx_rx = tx_rx.fuse().peekable();

        let chain_cfg = chain_cfg.clone();
        let miner_cfg = miner_cfg.clone();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                if Pin::new(&mut tx_rx).peek().await.is_none() {
                    return;
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
                    None => return,
                };

                raft_storage
                    .set_miner_snapshot(&blk_proposal, snapshot)
                    .await;

                {
                    let _guard = raft_client_lock.lock().await;
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
                                continue;
                            }
                        },
                        Err(ClientWriteError::ForwardToLeader(_, leader)) => {
                            warn!("Raft write should be forward to leader ({:?}).", leader);
                            continue;
                        }
                        Err(ClientWriteError::RaftError(e)) => {
                            error!("Raft write error from raft. Error: {}", e);
                            continue;
                        }
                    }
                }

                block_proposal_broadcast_tx.send(blk_proposal).await.ok();
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
        }
    }

    pub fn get_tx_tx(&self) -> mpsc::UnboundedSender<TxProposal<Tx>> {
        self.tx_tx.clone()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.tx_tx.close_channel();
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
