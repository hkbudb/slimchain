use super::BlockImportWorker;
use crate::{block_proposal::BlockProposal, snapshot::Snapshot};
use async_trait::async_trait;
use libp2p::{swarm::NetworkBehaviourEventProcess, NetworkBehaviour, PeerId};
use serde::Serialize;
use slimchain_chain::{
    config::ChainConfig, consensus::pow::Block, db::DBPtr, latest::LatestTxCount, role::Role,
};
use slimchain_common::{
    basic::{ShardId, H256},
    collections::HashMap,
    error::Result,
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
use slimchain_network::p2p::{
    config::NetworkConfig,
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent, QueryId as DiscoveryQueryId},
    http::{ClientHttpServer, TxHttpRequest},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
    rpc::{
        create_request_response_client, handle_request_response_client_event, RpcInstant,
        RpcRequestId, RpcRequestResponseEvent,
    },
};
use slimchain_utils::record_event;
use std::time::Duration;

#[derive(NetworkBehaviour)]
pub struct ClientBehavior<Tx: TxTrait + Serialize + 'static> {
    discv: Discovery,
    pubsub: PubSub<Tx, BlockProposal<Block, Tx>>,
    http_server: ClientHttpServer,
    rpc_client: RpcInstant<SignedTxRequest, ()>,
    #[behaviour(ignore)]
    worker: BlockImportWorker<Tx>,
    #[behaviour(ignore)]
    pending_discv_queries: HashMap<DiscoveryQueryId, SignedTxRequest>,
    #[behaviour(ignore)]
    pending_rpc_queries: HashMap<RpcRequestId, H256>,
}

impl<Tx: TxTrait + Serialize + 'static> ClientBehavior<Tx> {
    pub async fn new(db: DBPtr, chain_cfg: &ChainConfig, net_cfg: &NetworkConfig) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let peer_id = PeerId::from(keypair.public());

        let mut discv = Discovery::new(keypair.public(), Role::Client, net_cfg.mdns).await?;
        discv.add_address_from_net_config(net_cfg);
        let mut pubsub = PubSub::new(keypair, &[PubSubTopic::BlockProposal], &[])?;
        pubsub.add_peers_from_net_config(net_cfg);
        let mut rpc_client = create_request_response_client("/tx_req/1");

        for peer in &net_cfg.peers {
            if peer_id != peer.peer_id {
                rpc_client.add_address(&peer.peer_id, peer.address.clone());
            }
        }

        let snapshot = Snapshot::<Block>::load_from_db(&db, chain_cfg.state_len)?;
        let latest_block_header = snapshot.to_latest_block_header();
        let latest_tx_count = LatestTxCount::new(0);
        let worker = BlockImportWorker::new(
            chain_cfg.clone(),
            snapshot,
            latest_block_header.clone(),
            latest_tx_count.clone(),
            db,
        );

        let http_server = ClientHttpServer::new(
            &net_cfg.http_listen,
            move || latest_tx_count.get(),
            move || latest_block_header.get_height(),
        )?;

        Ok(Self {
            discv,
            pubsub,
            http_server,
            rpc_client,
            worker,
            pending_discv_queries: HashMap::new(),
            pending_rpc_queries: HashMap::new(),
        })
    }

    pub fn discv_mut(&mut self) -> &mut Discovery {
        &mut self.discv
    }

    pub fn pubsub_mut(&mut self) -> &mut PubSub<Tx, BlockProposal<Block, Tx>> {
        &mut self.pubsub
    }
}

impl<Tx: TxTrait + Serialize> NetworkBehaviourEventProcess<TxHttpRequest> for ClientBehavior<Tx> {
    fn inject_event(&mut self, tx_http_req: TxHttpRequest) {
        let TxHttpRequest { req, .. } = tx_http_req;
        trace!(tx_req_id = %req.id(), "Recv TxReq from http.");
        let discv_query_id = self
            .discv
            .find_random_peer(Role::Storage(ShardId::default()), Duration::from_secs(5));
        self.pending_discv_queries.insert(discv_query_id, req);
    }
}

impl<Tx: TxTrait + Serialize> NetworkBehaviourEventProcess<DiscoveryEvent> for ClientBehavior<Tx> {
    fn inject_event(&mut self, event: DiscoveryEvent) {
        if let DiscoveryEvent::FindPeerResult { query_id, peer } = event {
            let tx_req = match self.pending_discv_queries.remove(&query_id) {
                Some(req) => req,
                None => return,
            };
            let tx_req_id = tx_req.id();

            match peer {
                Ok(peer_id) => {
                    record_event!("tx_begin", "tx_id": tx_req.id());
                    let rpc_query_id = self.rpc_client.send_request(&peer_id, tx_req);
                    self.pending_rpc_queries.insert(rpc_query_id, tx_req_id);
                }
                Err(e) => {
                    error!(%tx_req_id, "Failed to find the storage node. Error: {}", e);
                }
            }
        }
    }
}

impl<Tx: TxTrait + Serialize>
    NetworkBehaviourEventProcess<RpcRequestResponseEvent<SignedTxRequest, ()>>
    for ClientBehavior<Tx>
{
    fn inject_event(&mut self, event: RpcRequestResponseEvent<SignedTxRequest, ()>) {
        let (rpc_query_id, result) = match handle_request_response_client_event(event) {
            Some(res) => res,
            None => return,
        };
        let tx_req_id = self
            .pending_rpc_queries
            .remove(&rpc_query_id)
            .expect("Cannot find tx_req_id");

        if let Err(e) = result {
            error!(
                %tx_req_id,
                "Storage node returns failure. Error: {}", e
            );
        }
    }
}

impl<Tx: TxTrait + Serialize>
    NetworkBehaviourEventProcess<PubSubEvent<Tx, BlockProposal<Block, Tx>>> for ClientBehavior<Tx>
{
    fn inject_event(&mut self, event: PubSubEvent<Tx, BlockProposal<Block, Tx>>) {
        if let PubSubEvent::BlockProposal(input) = event {
            trace!(
                height = input.get_block_height().0,
                txs = input.get_txs().len(),
                "Recv block proposal."
            );
            self.worker.add_block_proposal(input);
        }
    }
}

#[async_trait]
impl<Tx: TxTrait + Serialize> Shutdown for ClientBehavior<Tx> {
    async fn shutdown(&mut self) -> Result<()> {
        self.worker.shutdown().await
    }
}
