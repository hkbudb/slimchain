use super::BlockImportWorker;
use crate::{
    block::{pow::Block, BlockTrait},
    config::{NetworkConfig, Role},
    db::DBPtr,
};
use async_trait::async_trait;
use libp2p::{swarm::NetworkBehaviourEventProcess, NetworkBehaviour};
use slimchain_chain::latest::LatestTxCount;
use slimchain_common::{error::Result, tx_req::SignedTxRequest};
use slimchain_network::{
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent},
    http::{TxHttpRequest, TxHttpServer},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
};
use slimchain_utils::record_event;

#[derive(NetworkBehaviour)]
pub struct ClientBehavior {
    discv: Discovery,
    pubsub: PubSub<SignedTxRequest, Block>,
    http_server: TxHttpServer,
    #[behaviour(ignore)]
    worker: BlockImportWorker,
}

impl ClientBehavior {
    pub fn new(db: DBPtr, net_cfg: &NetworkConfig) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let mut discv = Discovery::new(keypair.public(), Role::Client, net_cfg.mdns)?;
        discv.add_address_from_net_config(net_cfg);
        let pubsub = PubSub::new(
            keypair,
            &[PubSubTopic::BlockProposal, PubSubTopic::TxProposal],
        );
        let height = db.get_block_height()?.unwrap_or_default();
        let latest_tx_count = LatestTxCount::new(0);
        let worker = BlockImportWorker::new(db.clone(), height, latest_tx_count.clone());

        let http_server = TxHttpServer::new(
            &net_cfg.http_listen,
            move || latest_tx_count.get(),
            move || {
                db.get_block_height()
                    .expect("Failed to get the block height.")
                    .unwrap_or_default()
            },
        )?;

        Ok(Self {
            discv,
            pubsub,
            http_server,
            worker,
        })
    }
}

impl NetworkBehaviourEventProcess<TxHttpRequest> for ClientBehavior {
    fn inject_event(&mut self, tx_http_req: TxHttpRequest) {
        let TxHttpRequest { req, .. } = tx_http_req;
        let tx_req_id = req.id();
        debug!(%tx_req_id, "Recv TxReq from http.");
        record_event!("tx_begin", "tx_id": tx_req_id);
        self.pubsub
            .publish_tx_proposal(&req)
            .expect("Failed to publish tx.");
    }
}

impl NetworkBehaviourEventProcess<DiscoveryEvent> for ClientBehavior {
    fn inject_event(&mut self, _event: DiscoveryEvent) {}
}

impl NetworkBehaviourEventProcess<PubSubEvent<SignedTxRequest, Block>> for ClientBehavior {
    fn inject_event(&mut self, event: PubSubEvent<SignedTxRequest, Block>) {
        if let PubSubEvent::BlockProposal(input) = event {
            debug!(
                height = input.block_height().0,
                txs = input.tx_list().len(),
                "Recv block proposal."
            );
            self.worker.add_block(input);
        }
    }
}

#[async_trait]
impl Shutdown for ClientBehavior {
    async fn shutdown(&mut self) -> Result<()> {
        self.worker.shutdown().await
    }
}
