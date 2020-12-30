use crate::{
    behavior::raft::{
        client_block_proposal::BlockProposalWorker,
        client_network::ClientNodeNetwork,
        client_storage::ClientNodeStorage,
        message::{NewBlockRequest, NewBlockResponse},
    },
    http::{
        client_rpc::*,
        common::*,
        config::{NetworkConfig, PeerId, RaftConfig},
        node_rpc::*,
    },
};
use async_raft::{
    error::{ClientReadError, InitializeError, RaftError},
    Raft,
};
use futures::{channel::oneshot, future::join_all, prelude::*, stream};
use serde::{Deserialize, Serialize};
use slimchain_chain::{
    config::{ChainConfig, MinerConfig},
    db::DBPtr,
};
use slimchain_common::{
    error::{anyhow, bail, Error, Result},
    tx::TxTrait,
};
use slimchain_tx_state::TxProposal;
use std::{net::SocketAddr, sync::Arc};
use tokio::task::JoinHandle;
use warp::Filter;

pub type ClientNodeRaft<Tx> =
    Raft<NewBlockRequest<Tx>, NewBlockResponse, ClientNodeNetwork<Tx>, ClientNodeStorage<Tx>>;

#[derive(Debug)]
enum ClientNodeError {
    RaftError(RaftError),
    Other(Error),
}

impl warp::reject::Reject for ClientNodeError {}

pub struct ClientNode<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> {
    raft_storage: Arc<ClientNodeStorage<Tx>>,
    raft: Option<Arc<ClientNodeRaft<Tx>>>,
    srv: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    proposal_worker: BlockProposalWorker<Tx>,
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> ClientNode<Tx> {
    pub async fn new(
        db: DBPtr,
        chain_cfg: &ChainConfig,
        miner_cfg: &MinerConfig,
        net_cfg: &NetworkConfig,
        raft_cfg: &RaftConfig,
    ) -> Result<Self> {
        let net_route_table = net_cfg.to_route_table();
        let peer_id = net_route_table.peer_id();
        let all_peers = net_route_table.all_client_peer_ids();

        let raft_storage = Arc::new(ClientNodeStorage::new(db, chain_cfg, net_cfg)?);
        let raft_network = Arc::new(ClientNodeNetwork::new(net_route_table)?);
        let raft = Arc::new(ClientNodeRaft::new(
            peer_id.into(),
            raft_cfg.to_raft_config()?,
            raft_network.clone(),
            raft_storage.clone(),
        ));

        let proposal_worker = BlockProposalWorker::new(
            chain_cfg,
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
                    async {
                        join_all(reqs.into_iter().map(move |req| {
                            let raft_network_copy = raft_network_copy.clone();
                            async move { raft_network_copy.forward_tx_to_storage_node(req).await }
                        }))
                        .await;
                        Ok(())
                    }
                },
                move || raft_storage_copy1.latest_tx_count().get(),
                move || raft_storage_copy2.latest_block_header().get_height(),
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
                        match raft_copy.client_read().await {
                            Ok(()) => Ok(peer_id),
                            Err(ClientReadError::ForwardToLeader(Some(id))) => Ok(PeerId::from(id)),
                            Err(ClientReadError::ForwardToLeader(None)) => {
                                Err(ClientNodeError::Other(anyhow!("Leader unknown")))
                            }
                            Err(ClientReadError::RaftError(e)) => {
                                Err(ClientNodeError::RaftError(e))
                            }
                        }
                        .map(|resp| warp_reply_postcard(&resp))
                        .map_err(warp::reject::custom)
                    }
                });

            let tx_tx = proposal_worker.get_tx_tx();
            let leader_req_rpc = warp::post()
                .and(warp::path(CLIENT_LEADER_REQ_ROUTE_PATH))
                .and(warp_body_postcard())
                .and_then(move |txs: Vec<TxProposal<Tx>>| {
                    let mut tx_tx_copy = tx_tx.clone();
                    let mut input = stream::iter(txs).map(|tx| {
                        trace!(
                            tx_id = %tx.tx.id(),
                            "Recv tx proposal."
                        );
                        Ok(tx)
                    });
                    async move {
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

        info!("Create http server, listen on {}", net_cfg.http_listen);
        let listen_addr: SocketAddr = net_cfg.http_listen.parse()?;
        let (srv_shutdown_tx, srv_shutdown_rx) = oneshot::channel::<()>();
        let (_, srv) = warp::serve(
            client_rpc_srv.or(warp::path(NODE_RPC_ROUTE_PATH).and(raft_rpc_srv.or(leader_rpc_srv))),
        )
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
