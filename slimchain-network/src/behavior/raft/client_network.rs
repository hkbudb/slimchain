use crate::{
    behavior::raft::message::NewBlockRequest,
    http::{
        client_rpc::TxHttpRequest,
        common::*,
        config::{NetworkRouteTable, PeerId},
        node_rpc::*,
    },
};
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
    future,
    prelude::*,
};
use serde::{Deserialize, Serialize};
use slimchain_chain::{block_proposal::BlockProposal, consensus::raft::Block, role::Role};
use slimchain_common::{
    error::{anyhow, bail, Result},
    tx::TxTrait,
};
use slimchain_tx_state::TxProposal;
use slimchain_utils::{bytes::Bytes, record_event, serde::binary_encode};
use std::{marker::PhantomData, sync::Arc};
use tokio::{sync::RwLock, task::JoinHandle};

pub async fn fetch_leader_id(route_table: &NetworkRouteTable) -> Result<PeerId> {
    let rand_client = route_table
        .random_peer(&Role::Client)
        .ok_or_else(|| anyhow!("Failed to find the client node."))
        .and_then(|peer_id| route_table.peer_address(peer_id))?;
    get_leader(rand_client).await
}

pub struct ClientNodeNetwork<Tx>
where
    Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static,
{
    route_table: NetworkRouteTable,
    leader_id: RwLock<Option<PeerId>>,
    _marker: PhantomData<Tx>,
}

impl<Tx> ClientNodeNetwork<Tx>
where
    Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static,
{
    pub fn new(route_table: NetworkRouteTable) -> Self {
        Self {
            route_table,
            leader_id: RwLock::new(None),
            _marker: PhantomData,
        }
    }

    #[tracing::instrument(level = "debug", skip(self, tx_req))]
    pub async fn forward_tx_to_storage_node(&self, tx_req: TxHttpRequest) {
        let TxHttpRequest { req, shard_id } = tx_req;
        let tx_req_id = req.id();

        let storage_node_peer_id = match self.route_table.random_peer(&Role::Storage(shard_id)) {
            Some(peer) => peer,
            None => {
                error!(%tx_req_id , "Failed to find the storage node. ShardId: {:?}", shard_id);
                return;
            }
        };
        debug_assert_ne!(storage_node_peer_id, self.route_table.peer_id());

        let storage_node_addr = match self.route_table.peer_address(storage_node_peer_id) {
            Ok(addr) => addr,
            Err(_) => {
                error!(%tx_req_id , "Failed to get the storage address. PeerId: {}", storage_node_peer_id);
                return;
            }
        };

        record_event!("tx_begin", "tx_id": tx_req_id);

        let resp: Result<()> = send_post_request_using_binary(
            &format!(
                "http://{}/{}/{}",
                storage_node_addr, NODE_RPC_ROUTE_PATH, STORAGE_TX_REQ_ROUTE_PATH
            ),
            &req,
        )
        .await;

        if let Err(e) = resp {
            error!(
                %tx_req_id,
                "Failed to forward TX to storage node. Error: {}", e
            );
        }
    }

    pub async fn set_leader(&self, leader_id: PeerId) {
        *self.leader_id.write().await = Some(leader_id);
    }

    #[allow(clippy::ptr_arg)]
    #[allow(clippy::unit_arg)]
    #[tracing::instrument(level = "debug", skip(self, tx_proposals), err)]
    pub async fn forward_tx_proposal_to_leader(
        &self,
        tx_proposals: &Vec<TxProposal<Tx>>,
    ) -> Result<()> {
        let leader_id = *self.leader_id.read().await;
        let leader_id = match leader_id {
            Some(id) => id,
            None => {
                let id = fetch_leader_id(&self.route_table).await?;
                *self.leader_id.write().await = Some(id);
                id
            }
        };

        debug_assert_ne!(leader_id, self.route_table.peer_id());
        let addr = self.route_table.peer_address(leader_id)?;
        match send_reqs_to_leader(addr, tx_proposals).await {
            Err(e) => {
                *self.leader_id.write().await = None;
                Err(e)
            }
            Ok(()) => Ok(()),
        }
    }

    #[allow(clippy::unit_arg)]
    #[allow(clippy::ptr_arg)]
    #[tracing::instrument(level = "debug", skip(self, block_proposals), err)]
    pub async fn broadcast_block_proposal_to_storage_node(
        &self,
        block_proposals: &Vec<BlockProposal<Block, Tx>>,
    ) -> Result<()> {
        if block_proposals.is_empty() {
            return Ok(());
        }

        let bytes = Bytes::from(binary_encode(block_proposals)?);
        let reqs = self
            .route_table
            .role_table()
            .iter()
            .filter(|(role, _)| matches!(role, Role::Storage(_)))
            .flat_map(|(_, list)| list.iter())
            .filter_map(|&peer_id| match self.route_table.peer_address(peer_id) {
                Ok(addr) => Some((
                    peer_id,
                    format!(
                        "http://{}/{}/{}",
                        addr, NODE_RPC_ROUTE_PATH, STORAGE_BLOCK_IMPORT_ROUTE_PATH
                    ),
                )),
                Err(_) => {
                    warn!("Failed to get the peer address. PeerId: {}", peer_id);
                    None
                }
            })
            .map(|(peer_id, uri)| {
                let bytes = bytes.clone();
                async move {
                    (
                        peer_id,
                        send_post_request_using_binary_bytes::<()>(&uri, bytes).await,
                    )
                }
            });

        for (peer_id, resp) in future::join_all(reqs).await {
            if let Err(e) = resp {
                let begin_block_height = block_proposals
                    .first()
                    .expect("empty block proposals")
                    .get_block_height();
                let end_block_height = block_proposals
                    .last()
                    .expect("empty block proposals")
                    .get_block_height();
                error!(%begin_block_height, %end_block_height, %peer_id, "Failed to broadcast block proposal to storage node. Err: {:?}", e);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<Tx> RaftNetwork<NewBlockRequest<Tx>> for ClientNodeNetwork<Tx>
where
    Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static,
{
    #[tracing::instrument(level = "debug", skip(self, rpc))]
    async fn append_entries(
        &self,
        target: NodeId,
        rpc: AppendEntriesRequest<NewBlockRequest<Tx>>,
    ) -> Result<AppendEntriesResponse> {
        let peer_id = PeerId::from(target);
        debug_assert_ne!(peer_id, self.route_table.peer_id());
        let addr = self.route_table.peer_address(peer_id)?;
        send_post_request_using_binary(
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
        send_post_request_using_binary(
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
        send_post_request_using_binary(
            &format!(
                "http://{}/{}/{}",
                addr, NODE_RPC_ROUTE_PATH, RAFT_VOTE_ROUTE_PATH
            ),
            &rpc,
        )
        .await
    }
}

pub struct ClientNodeNetworkWorker<Tx>
where
    Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static,
{
    req_handle: Option<JoinHandle<()>>,
    req_tx: mpsc::UnboundedSender<TxHttpRequest>,
    req_shutdown_tx: Option<oneshot::Sender<()>>,
    block_proposal_handle: Option<JoinHandle<()>>,
    block_proposal_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
    block_proposal_shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx> ClientNodeNetworkWorker<Tx>
where
    Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static,
{
    pub fn new(network: Arc<ClientNodeNetwork<Tx>>, async_broadcast_storage: bool) -> Self {
        let (req_tx, req_rx) = mpsc::unbounded();
        let req_fut = {
            let network = network.clone();
            req_rx.for_each_concurrent(64, move |req| {
                let network = network.clone();
                async move { network.forward_tx_to_storage_node(req).await }
            })
        };
        let (req_shutdown_tx, req_shutdown_rx) = oneshot::channel();

        let req_handle = tokio::spawn(async move {
            tokio::select! {
                _ = req_shutdown_rx => {}
                _ = req_fut => {}
            }
        });

        let (block_proposal_tx, block_proposal_rx) = mpsc::unbounded();
        let mut block_proposal_rx = block_proposal_rx.ready_chunks(8);
        let (block_proposal_shutdown_tx, mut block_proposal_shutdown_rx) = oneshot::channel();

        let block_proposal_handle = if async_broadcast_storage {
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut block_proposal_shutdown_rx => break,
                        Some(block_proposals) = block_proposal_rx.next() => {
                            network.broadcast_block_proposal_to_storage_node(&block_proposals).await.ok();
                        }
                    }
                }
            })
        } else {
            tokio::spawn(async {})
        };

        Self {
            req_handle: Some(req_handle),
            req_tx,
            req_shutdown_tx: Some(req_shutdown_tx),
            block_proposal_handle: Some(block_proposal_handle),
            block_proposal_tx,
            block_proposal_shutdown_tx: Some(block_proposal_shutdown_tx),
        }
    }

    pub fn get_req_tx(&self) -> mpsc::UnboundedSender<TxHttpRequest> {
        self.req_tx.clone()
    }

    pub fn get_block_proposal_tx(&self) -> mpsc::UnboundedSender<BlockProposal<Block, Tx>> {
        self.block_proposal_tx.clone()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.req_tx.close_channel();
        if let Some(shutdown_tx) = self.req_shutdown_tx.take() {
            shutdown_tx.send(()).ok();
        } else {
            bail!("Already shutdown.");
        }
        if let Some(handler) = self.req_handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }

        self.block_proposal_tx.close_channel();
        if let Some(shutdown_tx) = self.block_proposal_shutdown_tx.take() {
            shutdown_tx.send(()).ok();
        } else {
            bail!("Already shutdown.");
        }
        if let Some(handler) = self.block_proposal_handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
