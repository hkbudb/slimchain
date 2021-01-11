use super::BlockProposalWorker;
use crate::{block::pow::Block, db::DBPtr};
use async_trait::async_trait;
use libp2p::{
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour,
};
use slimchain_chain::{config::MinerConfig, latest::LatestTxCount, role::Role};
use slimchain_common::{error::Result, tx_req::SignedTxRequest};
use slimchain_network::p2p::{
    config::NetworkConfig,
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
};
use slimchain_utils::record_event;
use std::task::{Context, Poll};

#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll_inner")]
pub struct MinerBehavior {
    discv: Discovery,
    pubsub: PubSub<SignedTxRequest, Block>,
    #[behaviour(ignore)]
    worker: BlockProposalWorker,
}

impl MinerBehavior {
    pub async fn new(db: DBPtr, miner_cfg: &MinerConfig, net_cfg: &NetworkConfig) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let mut discv = Discovery::new(keypair.public(), Role::Miner, net_cfg.mdns).await?;
        discv.add_address_from_net_config(net_cfg);
        let pubsub = PubSub::new(keypair, &[PubSubTopic::TxProposal], &[])?;
        let height = db.get_meta_object("height")?.unwrap_or_default();
        let latest_tx_count = LatestTxCount::new(0);
        let worker = BlockProposalWorker::new(miner_cfg.clone(), db, height, latest_tx_count);

        Ok(Self {
            discv,
            pubsub,
            worker,
        })
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, ()>> {
        if let Poll::Ready(block) = self.worker.poll_block(cx) {
            self.pubsub
                .publish_block_proposal(&block)
                .expect("Failed to publish block.");
        }

        Poll::Pending
    }
}

impl NetworkBehaviourEventProcess<DiscoveryEvent> for MinerBehavior {
    fn inject_event(&mut self, _event: DiscoveryEvent) {}
}

impl NetworkBehaviourEventProcess<PubSubEvent<SignedTxRequest, Block>> for MinerBehavior {
    fn inject_event(&mut self, event: PubSubEvent<SignedTxRequest, Block>) {
        if let PubSubEvent::TxProposal(input) = event {
            record_event!("miner_recv_tx", "tx_id": input.id());
            self.worker.add_tx(input);
        }
    }
}

#[async_trait]
impl Shutdown for MinerBehavior {
    async fn shutdown(&mut self) -> Result<()> {
        self.worker.shutdown().await
    }
}
