use super::client_network::fetch_leader_id;
use crate::http::{
    common::*,
    config::{NetworkConfig, NetworkRouteTable, PeerId},
    node_rpc::*,
};
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use slimchain_chain::{
    behavior::{commit_block_storage_node, verify_block, TxExecuteStream},
    block_proposal::BlockProposal,
    config::ChainConfig,
    consensus::raft::{verify_consensus, Block},
    db::DBPtr,
    latest::{LatestBlockHeaderPtr, LatestTxCount, LatestTxCountPtr},
    snapshot::Snapshot,
};
use slimchain_common::{
    basic::ShardId,
    error::{bail, Result},
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
use slimchain_tx_engine::TxEngine;
use slimchain_tx_state::{StorageTxTrie, TxProposal};
use slimchain_utils::{ordered_stream::OrderedStream, record_event};
use std::{
    marker::PhantomData,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{sync::RwLock, task::JoinHandle};
use warp::Filter;

const MAX_RETRIES: usize = 3;

struct SendToLeader<Tx: TxTrait + Serialize> {
    route_table: NetworkRouteTable,
    leader_id: RwLock<Option<PeerId>>,
    _marker: PhantomData<Tx>,
}

impl<Tx: TxTrait + Serialize> SendToLeader<Tx> {
    fn new(route_table: NetworkRouteTable) -> Self {
        Self {
            route_table,
            leader_id: RwLock::new(None),
            _marker: PhantomData,
        }
    }

    #[allow(clippy::ptr_arg)]
    async fn send_tx_proposals(&self, tx_proposals: &Vec<TxProposal<Tx>>) -> Result<()> {
        let leader_id = *self.leader_id.read().await;
        let leader_id = match leader_id {
            Some(id) => id,
            None => {
                let id = fetch_leader_id(&self.route_table).await?;
                *self.leader_id.write().await = Some(id);
                id
            }
        };

        let leader_addr = self.route_table.peer_address(leader_id)?;
        match send_reqs_to_leader(leader_addr, tx_proposals).await {
            Err(e) => {
                *self.leader_id.write().await = None;
                Err(e)
            }
            Ok(()) => Ok(()),
        }
    }
}

struct TxExecWorker {
    tx_req_tx: mpsc::UnboundedSender<SignedTxRequest>,
    engine_shutdown_token: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl TxExecWorker {
    fn new<Tx: TxTrait + Serialize>(
        route_table: NetworkRouteTable,
        engine: TxEngine<Tx>,
        db: &DBPtr,
        latest_block_header: &LatestBlockHeaderPtr,
    ) -> Self {
        let send_to_leader = Arc::new(SendToLeader::new(route_table));
        let engine_shutdown_token = engine.shutdown_token();
        let (tx_req_tx, tx_req_rx) = mpsc::unbounded::<SignedTxRequest>();
        let tx_exec_fut = TxExecuteStream::new(tx_req_rx, engine, &db, &latest_block_header)
            .ready_chunks(8)
            .for_each_concurrent(8, move |tx_proposals| {
                let send_to_leader = send_to_leader.clone();
                async move {
                    for i in 1..=MAX_RETRIES {
                        match send_to_leader.send_tx_proposals(&tx_proposals).await {
                            Ok(_) => break,
                            Err(e) => {
                                if i == MAX_RETRIES {
                                    error!(
                                        "Failed to send tx_proposal to raft leader. Error: {}",
                                        e
                                    );
                                    for tx in &tx_proposals {
                                        let tx_id = tx.tx.id();
                                        record_event!("discard_tx", "tx_id": tx_id, "reason": "storage_send_to_leader", "detail": std::format!("{}", e));
                                    }
                                }
                            }
                        }
                    }
                }
            });
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            tokio::select! {
                _ = shutdown_rx => {}
                _ = tx_exec_fut => {}
            }
        });

        Self {
            tx_req_tx,
            engine_shutdown_token,
            handle: Some(handle),
            shutdown_tx: Some(shutdown_tx),
        }
    }

    fn get_tx_req_tx(&self) -> mpsc::UnboundedSender<SignedTxRequest> {
        self.tx_req_tx.clone()
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.tx_req_tx.close_channel();
        self.engine_shutdown_token.store(true, Ordering::Release);
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

struct BlockImportWorker<Tx: TxTrait + 'static> {
    handle: Option<JoinHandle<()>>,
    blk_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx: TxTrait + Serialize> BlockImportWorker<Tx> {
    fn new(
        chain_cfg: ChainConfig,
        mut snapshot: Snapshot<Block, StorageTxTrie>,
        latest_block_header: LatestBlockHeaderPtr,
        latest_tx_count: LatestTxCountPtr,
        db: DBPtr,
    ) -> Self {
        let (blk_tx, blk_rx) = mpsc::unbounded::<BlockProposal<Block, Tx>>();
        let mut blk_rx = OrderedStream::new(
            blk_rx.map(|blk| (blk.get_block_height(), blk)),
            latest_block_header.get_height().next_height(),
            |height| height.next_height(),
        );
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    Some(blk_proposal) = blk_rx.next() => {
                        let state_update = {
                            let snapshot_backup = snapshot.clone();
                            match verify_block(&chain_cfg, &mut snapshot, &blk_proposal, verify_consensus)
                                .await
                            {
                                Ok(state_update) => state_update,
                                Err(e) => {
                                    snapshot = snapshot_backup;
                                    error!("Failed to import block. Error: {}", e);
                                    continue;
                                }
                            }
                        };

                        if let Err(e) = commit_block_storage_node(
                            &blk_proposal,
                            &state_update,
                            &db,
                            &latest_block_header,
                            &latest_tx_count,
                        )
                        .await
                        {
                            if let Ok(db_tx) = snapshot.write_db_tx() {
                                db.write_async(db_tx).await.ok();
                            }
                            panic!("Failed to commit the block. Error: {}", e);
                        }
                    }
                }
            }

            db.write_async(
                snapshot
                    .write_db_tx()
                    .expect("Failed to save the snapshot."),
            )
            .await
            .expect("Failed to save the snapshot.");
        });

        Self {
            handle: Some(handle),
            blk_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    fn get_blk_tx(&self) -> mpsc::UnboundedSender<BlockProposal<Block, Tx>> {
        self.blk_tx.clone()
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.blk_tx.close_channel();
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

#[derive(Debug)]
struct StorageNodeReqError(mpsc::SendError);

impl warp::reject::Reject for StorageNodeReqError {}

pub struct StorageNode<Tx: TxTrait + 'static> {
    srv: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    exec_worker: TxExecWorker,
    import_worker: BlockImportWorker<Tx>,
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> StorageNode<Tx> {
    pub async fn new(
        db: DBPtr,
        engine: TxEngine<Tx>,
        shard_id: ShardId,
        chain_cfg: &ChainConfig,
        net_cfg: &NetworkConfig,
    ) -> Result<Self> {
        let snapshot =
            Snapshot::<Block, StorageTxTrie>::load_from_db(&db, chain_cfg.state_len, shard_id)?;
        let latest_block_header = snapshot.to_latest_block_header();
        let latest_tx_count = LatestTxCount::new(0);

        let exec_worker =
            TxExecWorker::new(net_cfg.to_route_table(), engine, &db, &latest_block_header);
        let exec_worker_tx_req_tx = exec_worker.get_tx_req_tx();

        let import_worker = BlockImportWorker::new(
            chain_cfg.clone(),
            snapshot,
            latest_block_header,
            latest_tx_count,
            db,
        );
        let import_worker_blk_tx = import_worker.get_blk_tx();

        let tx_exec_srv = warp::post()
            .and(warp::path(STORAGE_TX_REQ_ROUTE_PATH))
            .and(warp_body_postcard())
            .and_then(move |req: SignedTxRequest| {
                record_event!("storage_recv_tx", "tx_id": req.id());
                let mut exec_worker_tx_req_tx = exec_worker_tx_req_tx.clone();
                async move {
                    exec_worker_tx_req_tx
                        .send(req)
                        .await
                        .map(|_| warp_reply_postcard(&()))
                        .map_err(|e| warp::reject::custom(StorageNodeReqError(e)))
                }
            });

        let block_import_srv = warp::post()
            .and(warp::path(STORAGE_BLOCK_IMPORT_ROUTE_PATH))
            .and(warp_body_postcard())
            .and_then(move |block_proposal: BlockProposal<Block, Tx>| {
                let mut import_worker_blk_tx = import_worker_blk_tx.clone();
                async move {
                    import_worker_blk_tx
                        .send(block_proposal)
                        .await
                        .map(|_| warp_reply_postcard(&()))
                        .map_err(|e| warp::reject::custom(StorageNodeReqError(e)))
                }
            });

        info!("Create http server, listen on {}", net_cfg.http_listen);
        let listen_addr: SocketAddr = net_cfg.http_listen.parse()?;
        let (srv_shutdown_tx, srv_shutdown_rx) = oneshot::channel::<()>();
        let (_, srv) =
            warp::serve(warp::path(NODE_RPC_ROUTE_PATH).and(tx_exec_srv.or(block_import_srv)))
                .bind_with_graceful_shutdown(listen_addr, async {
                    srv_shutdown_rx.await.ok();
                });
        let srv_handle = tokio::spawn(srv);

        Ok(Self {
            srv: Some((srv_shutdown_tx, srv_handle)),
            exec_worker,
            import_worker,
        })
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down TxExecWorker...");
        self.exec_worker.shutdown().await?;
        info!("Shutting down BlockImportWorker...");
        self.import_worker.shutdown().await?;
        info!("Shutting down HTTP Server...");
        if let Some((shutdown_tx, handler)) = self.srv.take() {
            shutdown_tx.send(()).ok();
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
