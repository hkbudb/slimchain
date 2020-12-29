use crate::http::{
    common::*,
    config::{NetworkConfig, NetworkRouteTable},
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
    role::Role,
    snapshot::Snapshot,
};
use slimchain_common::{
    basic::ShardId,
    error::{anyhow, bail, Result},
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
use slimchain_tx_engine::TxEngine;
use slimchain_tx_state::{StorageTxTrie, TxProposal};
use slimchain_utils::ordered_stream::OrderedStream;
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use warp::Filter;

struct TxExecWorker {
    tx_req_tx: mpsc::UnboundedSender<SignedTxRequest>,
    handle: Option<JoinHandle<()>>,
}

impl TxExecWorker {
    fn new<Tx: TxTrait + Serialize>(
        route_table: NetworkRouteTable,
        engine: TxEngine<Tx>,
        db: &DBPtr,
        latest_block_header: &LatestBlockHeaderPtr,
    ) -> Self {
        let (tx_req_tx, tx_req_rx) = mpsc::unbounded::<SignedTxRequest>();
        let mut tx_exec_stream = TxExecuteStream::new(tx_req_rx, engine, &db, &latest_block_header);

        let handle: JoinHandle<()> = tokio::spawn(async move {
            'outer: while let Some(tx_proposal) = tx_exec_stream.next().await {
                let tx_proposals = vec![tx_proposal];

                async fn inner<Tx: TxTrait + Serialize>(
                    route_table: &NetworkRouteTable,
                    tx_proposals: &Vec<TxProposal<Tx>>,
                ) -> Result<()> {
                    let rand_client = route_table
                        .random_peer(&Role::Client)
                        .ok_or_else(|| anyhow!("Failed to find the client node."))
                        .and_then(|peer_id| route_table.peer_address(peer_id))?;
                    let leader_peer_id = get_leader(rand_client).await?;
                    let leader_addr = route_table.peer_address(leader_peer_id)?;
                    send_reqs_to_leader(leader_addr, tx_proposals).await
                }

                const MAX_RETRIES: i32 = 3;

                for i in 1..=MAX_RETRIES {
                    match inner(&route_table, &tx_proposals).await {
                        Ok(_) => continue 'outer,
                        Err(e) => {
                            if i == MAX_RETRIES {
                                error!("Failed to send tx_proposal to raft leader. Error: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Self {
            tx_req_tx,
            handle: Some(handle),
        }
    }

    fn get_tx_req_tx(&self) -> mpsc::UnboundedSender<SignedTxRequest> {
        self.tx_req_tx.clone()
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.tx_req_tx.close_channel();
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

        let handle: JoinHandle<()> = tokio::spawn(async move {
            while let Some(blk_proposal) = blk_rx.next().await {
                let snapshot_backup = snapshot.clone();
                let state_update =
                    match verify_block(&chain_cfg, &mut snapshot, &blk_proposal, verify_consensus)
                        .await
                    {
                        Ok(state_update) => state_update,
                        Err(e) => {
                            snapshot = snapshot_backup;
                            error!("Failed to import block. Error: {}", e);
                            continue;
                        }
                    };
                std::mem::drop(snapshot_backup);

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
        }
    }

    fn get_blk_tx(&self) -> mpsc::UnboundedSender<BlockProposal<Block, Tx>> {
        self.blk_tx.clone()
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.blk_tx.close_channel();
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
        self.exec_worker.shutdown().await?;
        self.import_worker.shutdown().await?;
        if let Some((shutdown_tx, handler)) = self.srv.take() {
            shutdown_tx.send(()).ok();
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
