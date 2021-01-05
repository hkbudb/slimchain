use super::{
    client_block_proposal::BlockProposalWorker,
    client_network::ClientNodeNetwork,
    client_storage::ClientNodeStorage,
    message::{NewBlockRequest, NewBlockResponse},
    rpc::NODE_BLOCK_ROUTE_PATH,
};
use crate::db::DBPtr;
use async_raft::{
    error::{InitializeError, RaftError},
    Raft,
};
use futures::{channel::oneshot, prelude::*, stream};
use slimchain_chain::config::MinerConfig;
use slimchain_common::{
    basic::BlockHeight,
    error::{anyhow, bail, Error, Result},
    tx_req::SignedTxRequest,
};
use slimchain_network::{
    behavior::raft::utils::{get_current_leader, node_is_leader},
    http::{
        client_rpc::*,
        common::*,
        config::{NetworkConfig, RaftConfig},
        node_rpc::*,
    },
};
use std::{net::SocketAddr, sync::Arc};
use tokio::task::JoinHandle;
use warp::Filter;

pub type ClientNodeRaft =
    Raft<NewBlockRequest, NewBlockResponse, ClientNodeNetwork, ClientNodeStorage>;

#[derive(Debug)]
enum ClientNodeError {
    RaftError(RaftError),
    Other(Error),
}

impl warp::reject::Reject for ClientNodeError {}

pub struct ClientNode {
    raft_storage: Arc<ClientNodeStorage>,
    raft: Option<Arc<ClientNodeRaft>>,
    srv: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    proposal_worker: BlockProposalWorker,
}

impl ClientNode {
    pub async fn new(
        db: DBPtr,
        miner_cfg: &MinerConfig,
        net_cfg: &NetworkConfig,
        raft_cfg: &RaftConfig,
    ) -> Result<Self> {
        let net_route_table = net_cfg.to_route_table();
        let peer_id = net_route_table.peer_id();
        let all_peers = net_route_table.all_client_peer_ids();

        let raft_storage = Arc::new(ClientNodeStorage::new(db, net_cfg)?);
        let raft_network = Arc::new(ClientNodeNetwork::new(net_route_table));
        let raft = Arc::new(ClientNodeRaft::new(
            peer_id.into(),
            raft_cfg.to_raft_config()?,
            raft_network.clone(),
            raft_storage.clone(),
        ));

        let proposal_worker = BlockProposalWorker::new(
            miner_cfg,
            raft_storage.clone(),
            raft_network.clone(),
            raft.clone(),
        );

        let client_rpc_srv = {
            let raft_storage_copy1 = raft_storage.clone();
            let raft_storage_copy2 = raft_storage.clone();
            let raft_network_copy = raft_network.clone();
            client_rpc_server(
                move |reqs: Vec<TxHttpRequest>| {
                    let raft_network_copy = raft_network_copy.clone();
                    async move {
                        raft_network_copy.forward_tx_http_reqs_to_leader(reqs).await;
                        Ok(())
                    }
                },
                move || raft_storage_copy1.latest_tx_count().get(),
                move || {
                    raft_storage_copy2
                        .db()
                        .get_meta_object("height")
                        .expect("Failed to get the block height.")
                        .unwrap_or_default()
                },
            )
        };

        let raft_rpc_srv = {
            let raft_copy = raft.clone();
            let append_rpc = warp::post()
                .and(warp::path(RAFT_APPEND_ENTRIES_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |rpc| {
                    let raft_copy = raft_copy.clone();
                    async move {
                        raft_copy
                            .append_entries(rpc)
                            .await
                            .map(|resp| warp_reply_postcard(&resp))
                            .map_err(|e| warp::reject::custom(ClientNodeError::RaftError(e)))
                    }
                });

            let raft_copy = raft.clone();
            let install_rpc = warp::post()
                .and(warp::path(RAFT_INSTALL_SNAPSHOT_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |rpc| {
                    let raft_copy = raft_copy.clone();
                    async move {
                        raft_copy
                            .install_snapshot(rpc)
                            .await
                            .map(|resp| warp_reply_postcard(&resp))
                            .map_err(|e| warp::reject::custom(ClientNodeError::RaftError(e)))
                    }
                });

            let raft_copy = raft.clone();
            let vote_rpc = warp::post()
                .and(warp::path(RAFT_VOTE_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |rpc| {
                    let raft_copy = raft_copy.clone();
                    async move {
                        raft_copy
                            .vote(rpc)
                            .await
                            .map(|resp| warp_reply_postcard(&resp))
                            .map_err(|e| warp::reject::custom(ClientNodeError::RaftError(e)))
                    }
                });

            append_rpc.or(install_rpc).or(vote_rpc)
        };

        let leader_rpc_srv = {
            let raft_copy = raft.clone();
            let leader_id_rpc = warp::get()
                .and(warp::path(CLIENT_LEADER_ID_ROUTE_PATH))
                .and_then(move || {
                    let raft_copy = raft_copy.clone();
                    async move {
                        get_current_leader(raft_copy.as_ref())
                            .await
                            .map(|resp| warp_reply_postcard(&resp))
                            .map_err(|e| warp::reject::custom(ClientNodeError::Other(e)))
                    }
                });

            let raft_copy = raft.clone();
            let tx_tx = proposal_worker.get_tx_tx();
            let leader_req_rpc = warp::post()
                .and(warp::path(CLIENT_LEADER_REQ_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |txs: Vec<SignedTxRequest>| {
                    let raft_copy = raft_copy.clone();
                    let mut tx_tx_copy = tx_tx.clone();
                    let mut input = stream::iter(txs).map(|tx| {
                        trace!(
                            tx_id = %tx.id(),
                            "Recv tx proposal."
                        );
                        Ok(tx)
                    });
                    async move {
                        if !node_is_leader(raft_copy.as_ref()) {
                            return Err(warp::reject::custom(ClientNodeError::Other(anyhow!(
                                "not leader"
                            ))));
                        }

                        tx_tx_copy
                            .send_all(&mut input)
                            .await
                            .map(|_| warp_reply_postcard(&()))
                            .map_err(|e| {
                                warp::reject::custom(ClientNodeError::Other(Error::msg(e)))
                            })
                    }
                });

            leader_id_rpc.or(leader_req_rpc)
        };

        let block_rpc_srv = {
            let raft_storage_copy = raft_storage.clone();
            warp::post()
                .and(warp::path(NODE_BLOCK_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |height: BlockHeight| {
                    let raft_storage_copy = raft_storage_copy.clone();
                    async move {
                        raft_storage_copy
                            .get_block(height)
                            .await
                            .map(|resp| warp_reply_postcard(&resp))
                            .map_err(|e| warp::reject::custom(ClientNodeError::Other(e)))
                    }
                })
        };

        info!("Create http server, listen on {}", net_cfg.http_listen);
        let listen_addr: SocketAddr = net_cfg.http_listen.parse()?;
        let (srv_shutdown_tx, srv_shutdown_rx) = oneshot::channel::<()>();
        let (_, srv) = warp::serve(client_rpc_srv.or(
            warp::path(NODE_RPC_ROUTE_PATH).and(raft_rpc_srv.or(leader_rpc_srv).or(block_rpc_srv)),
        ))
        .bind_with_graceful_shutdown(listen_addr, async {
            srv_shutdown_rx.await.ok();
        });
        let srv_handle = tokio::spawn(srv);

        info!("Initialize Raft Node");
        match raft.initialize(all_peers).await {
            Ok(_) | Err(InitializeError::NotAllowed) => {}
            Err(e) => return Err(Error::from(e)),
        }

        Ok(Self {
            raft_storage,
            raft: Some(raft),
            srv: Some((srv_shutdown_tx, srv_handle)),
            proposal_worker,
        })
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(raft) = self.raft.take() {
            raft.shutdown().await?;
        } else {
            bail!("Already shutdown.");
        }

        self.raft_storage.save_to_db().await?;

        self.proposal_worker.shutdown().await?;

        if let Some((shutdown_tx, handler)) = self.srv.take() {
            shutdown_tx.send(()).ok();
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }

        Ok(())
    }
}
