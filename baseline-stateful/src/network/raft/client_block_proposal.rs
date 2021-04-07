use crate::{
    behavior::propose_block,
    block_proposal::BlockProposal,
    network::raft::{
        client::ClientNodeRaft,
        client_network::ClientNodeNetwork,
        client_storage::ClientNodeStorage,
        message::{NewBlockRequest, NewBlockResponse},
    },
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
    config::{ChainConfig, MinerConfig},
    consensus::raft::{create_new_block, Block},
    db::DBPtr,
};
use slimchain_common::{
    error::{bail, Result},
    tx::TxTrait,
};
use slimchain_utils::record_event;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct BlockProposalWorker<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<Tx>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> BlockProposalWorker<Tx> {
    pub fn new(
        db: DBPtr,
        chain_cfg: &ChainConfig,
        miner_cfg: &MinerConfig,
        raft_storage: Arc<ClientNodeStorage<Tx>>,
        raft_network: Arc<ClientNodeNetwork<Tx>>,
        raft: Arc<ClientNodeRaft<Tx>>,
        mut block_proposal_broadcast_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
        async_broadcast_storage: bool,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<Tx>();
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
                let blk_proposal_result = match propose_block(
                    &chain_cfg,
                    &miner_cfg,
                    &db,
                    &mut snapshot,
                    &mut tx_rx,
                    create_new_block,
                )
                .await
                {
                    Ok(blk_proposal_result) => blk_proposal_result,
                    Err(e) => {
                        error!("Failed to build the new block. Error: {}", e);
                        continue;
                    }
                };

                let (blk_proposal, state_update) = match blk_proposal_result {
                    Some(blk_proposal) => blk_proposal,
                    None => break,
                };

                raft_storage
                    .set_miner_snapshot(&blk_proposal, snapshot, state_update)
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
                        error!("Raft write should be forward to leader ({:?}).", leader);
                        raft_storage.reset_miner_snapshot().await;

                        for tx in blk_proposal.get_txs() {
                            let tx_id = tx.id();
                            record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_non_leader", "detail": std::format!("leader={:?}", leader));
                        }

                        if let Some(leader_id) = leader {
                            raft_network.set_leader(leader_id.into()).await;
                        }

                        let mut txs = Vec::with_capacity(tx_rx.size_hint().0);

                        while let Some(Some(tx)) = tx_rx.next().now_or_never() {
                            txs.push(tx);
                        }

                        if let Err(e) = raft_network.forward_tx_to_leader(&txs).await {
                            error!("Failed to forward buffered tx to leader. Error: {}", e);

                            for tx in txs {
                                let tx_id = tx.id();
                                record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_forward_leader_error");
                            }
                        }

                        continue;
                    }
                    Err(ClientWriteError::RaftError(e)) => {
                        error!("Raft write error from raft. Error: {}", e);
                        raft_storage.reset_miner_snapshot().await;

                        for tx in blk_proposal.get_txs() {
                            let tx_id = tx.id();
                            record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_error", "detail": std::format!("{}", e));
                        }

                        while let Some(Some(tx)) = tx_rx.next().now_or_never() {
                            let tx_id = tx.id();
                            record_event!("discard_tx", "tx_id": tx_id, "reason": "raft_write_error_buffered_tx");
                        }

                        continue;
                    }
                }

                if async_broadcast_storage {
                    block_proposal_broadcast_tx.send(blk_proposal).await.ok();
                } else {
                    raft_network
                        .broadcast_block_proposal_to_storage_node(&vec![blk_proposal])
                        .await
                        .ok();
                }
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn get_tx_tx(&self) -> mpsc::UnboundedSender<Tx> {
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
