use super::{
    client::ClientNodeRaft,
    client_network::ClientNodeNetwork,
    client_storage::ClientNodeStorage,
    message::{NewBlockRequest, NewBlockResponse},
};
use crate::{behavior::propose_block, block::raft::create_new_block};
use async_raft::{
    error::ClientWriteError,
    raft::{ClientWriteRequest, ClientWriteResponse},
};
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use slimchain_chain::config::MinerConfig;
use slimchain_common::{
    error::{bail, Result},
    tx_req::SignedTxRequest,
};
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

pub struct BlockProposalWorker {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<SignedTxRequest>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl BlockProposalWorker {
    pub fn new(
        miner_cfg: &MinerConfig,
        raft_storage: Arc<ClientNodeStorage>,
        _raft_network: Arc<ClientNodeNetwork>,
        raft: Arc<ClientNodeRaft>,
        raft_client_lock: Arc<Mutex<()>>,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<SignedTxRequest>();
        let mut tx_rx = tx_rx.fuse().peekable();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let miner_cfg = miner_cfg.clone();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = Pin::new(&mut tx_rx).peek() => {
                        if res.is_none() {
                            break;
                        }
                    }
                    _ = &mut shutdown_rx => break,
                }

                let blk_proposal = match propose_block(
                    &miner_cfg,
                    &raft_storage.db(),
                    raft_storage.latest_block_height().await,
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

                let (blk, update) = match blk_proposal {
                    Some(blk_proposal) => blk_proposal,
                    None => break,
                };

                raft_storage.set_miner_update(&blk, update).await;

                {
                    let _guard = raft_client_lock.lock().await;
                    match raft
                        .client_write(ClientWriteRequest::new(NewBlockRequest(blk.clone())))
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
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn get_tx_tx(&self) -> mpsc::UnboundedSender<SignedTxRequest> {
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
