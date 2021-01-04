use super::message::NewBlockRequest;
use async_raft::{
    raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    },
    NodeId, RaftNetwork,
};
use async_trait::async_trait;
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use slimchain_common::{
    error::{bail, Result},
    tx_req::SignedTxRequest,
};
use slimchain_network::{
    behavior::raft::storage::fetch_leader_id,
    http::{
        client_rpc::TxHttpRequest,
        common::*,
        config::{NetworkRouteTable, PeerId},
        node_rpc::*,
    },
};
use slimchain_utils::record_event;
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

const MAX_RETRIES: usize = 3;

pub struct ClientNodeNetwork {
    route_table: NetworkRouteTable,
    leader_id_cache: Mutex<Option<PeerId>>,
}

impl ClientNodeNetwork {
    pub fn new(route_table: NetworkRouteTable) -> Self {
        Self {
            route_table,
            leader_id_cache: Mutex::new(None),
        }
    }

    pub async fn forward_tx_http_reqs_to_leader(&self, tx_reqs: Vec<TxHttpRequest>) {
        'outer: for tx_req in tx_reqs {
            let TxHttpRequest { req, .. } = tx_req;

            let tx_req_id = req.id();
            trace!(%tx_req_id, "Recv TxReq from http.");
            record_event!("tx_begin", "tx_id": tx_req_id);
            let reqs = vec![req];

            for i in 1..=MAX_RETRIES {
                match self.forward_tx_reqs_to_leader(&reqs).await {
                    Ok(_) => continue 'outer,
                    Err(e) => {
                        *self.leader_id_cache.lock().await = None;
                        if i == MAX_RETRIES {
                            error!("Failed to send tx_req to raft leader. Error: {}", e);
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::ptr_arg)]
    pub async fn forward_tx_reqs_to_leader(&self, reqs: &Vec<SignedTxRequest>) -> Result<()> {
        let mut leader_id_cache = self.leader_id_cache.lock().await;
        let leader_id = match leader_id_cache.as_ref() {
            Some(id) => *id,
            None => {
                let id = fetch_leader_id(&self.route_table).await?;
                *leader_id_cache = Some(id);
                id
            }
        };

        let leader_addr = self.route_table.peer_address(leader_id)?;
        send_reqs_to_leader(leader_addr, &reqs).await
    }
}

#[async_trait]
impl RaftNetwork<NewBlockRequest> for ClientNodeNetwork {
    #[tracing::instrument(level = "debug", skip(self, rpc))]
    async fn append_entries(
        &self,
        target: NodeId,
        rpc: AppendEntriesRequest<NewBlockRequest>,
    ) -> Result<AppendEntriesResponse> {
        let peer_id = PeerId::from(target);
        debug_assert_ne!(peer_id, self.route_table.peer_id());
        let addr = self.route_table.peer_address(peer_id)?;
        send_post_request_using_postcard(
            &format!(
                "http://{}/{}/{}",
                addr, NODE_RPC_ROUTE_PATH, RAFT_APPEND_ENTRIES_ROUTE_PATH
            ),
            &rpc,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self, rpc))]
    async fn install_snapshot(
        &self,
        target: NodeId,
        rpc: InstallSnapshotRequest,
    ) -> Result<InstallSnapshotResponse> {
        let peer_id = PeerId::from(target);
        debug_assert_ne!(peer_id, self.route_table.peer_id());
        let addr = self.route_table.peer_address(peer_id)?;
        send_post_request_using_postcard(
            &format!(
                "http://{}/{}/{}",
                addr, NODE_RPC_ROUTE_PATH, RAFT_INSTALL_SNAPSHOT_ROUTE_PATH
            ),
            &rpc,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self, rpc))]
    async fn vote(&self, target: NodeId, rpc: VoteRequest) -> Result<VoteResponse> {
        let peer_id = PeerId::from(target);
        debug_assert_ne!(peer_id, self.route_table.peer_id());
        let addr = self.route_table.peer_address(peer_id)?;
        send_post_request_using_postcard(
            &format!(
                "http://{}/{}/{}",
                addr, NODE_RPC_ROUTE_PATH, RAFT_VOTE_ROUTE_PATH
            ),
            &rpc,
        )
        .await
    }
}

pub struct ClientNodeNetworkWorker {
    handle: Option<JoinHandle<()>>,
    req_tx: mpsc::UnboundedSender<Vec<TxHttpRequest>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl ClientNodeNetworkWorker {
    pub fn new(network: Arc<ClientNodeNetwork>) -> Self {
        let (req_tx, req_rx) = mpsc::unbounded();
        let mut req_rx = req_rx.fuse();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    req = req_rx.next() => {
                        if let Some(req) = req {
                            network.forward_tx_http_reqs_to_leader(req).await;
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        Self {
            handle: Some(handle),
            req_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn get_req_tx(&self) -> mpsc::UnboundedSender<Vec<TxHttpRequest>> {
        self.req_tx.clone()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.req_tx.close_channel();
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
