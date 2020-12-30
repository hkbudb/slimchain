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
use slimchain_network::p2p::{
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent},
    http::{ClientHttpServer, TxHttpRequest},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
};
use slimchain_utils::record_event;

#[derive(NetworkBehaviour)]
pub struct ClientBehavior {
    discv: Discovery,
    pubsub: PubSub<SignedTxRequest, Block>,
    http_server: ClientHttpServer,
    #[behaviour(ignore)]
    worker: BlockImportWorker,
}

impl ClientBehavior {
    pub async fn new(db: DBPtr, net_cfg: &NetworkConfig) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let mut discv = Discovery::new(keypair.public(), Role::Client, net_cfg.mdns).await?;
        discv.add_address_from_net_config(net_cfg);
        let pubsub = PubSub::new(keypair, &[PubSubTopic::BlockProposal], &[]);
        let height = db.get_meta_object("height")?.unwrap_or_default();
        let latest_tx_count = LatestTxCount::new(0);
        let worker = BlockImportWorker::new(db.clone(), height, latest_tx_count.clone());

        let http_server = ClientHttpServer::new(
            &net_cfg.http_listen,
            move || latest_tx_count.get(),
            move || {
                db.get_meta_object("height")
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

    pub fn discv_mut(&mut self) -> &mut Discovery {
        &mut self.discv
    }
}

impl NetworkBehaviourEventProcess<TxHttpRequest> for ClientBehavior {
    fn inject_event(&mut self, tx_http_req: TxHttpRequest) {
        let TxHttpRequest { req, .. } = tx_http_req;
        let tx_req_id = req.id();
        trace!(%tx_req_id, "Recv TxReq from http.");
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
            trace!(
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
