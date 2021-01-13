use super::BlockProposalWorker;
use crate::p2p::{
    config::NetworkConfig,
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
};
use async_trait::async_trait;
use libp2p::{
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour,
};
use serde::Serialize;
use slimchain_chain::{
    block_proposal::BlockProposal,
    config::{ChainConfig, MinerConfig},
    consensus::pow::Block,
    db::DBPtr,
    latest::LatestTxCount,
    role::Role,
    snapshot::Snapshot,
};
use slimchain_common::{error::Result, tx::TxTrait};
use slimchain_tx_state::{TxProposal, TxTrie};
use slimchain_utils::record_event;
use std::task::{Context, Poll};

#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll_inner")]
pub struct MinerBehavior<Tx: TxTrait + Serialize + 'static> {
    discv: Discovery,
    pubsub: PubSub<TxProposal<Tx>, BlockProposal<Block, Tx>>,
    #[behaviour(ignore)]
    worker: BlockProposalWorker<Tx>,
}

impl<Tx: TxTrait + Serialize + 'static> MinerBehavior<Tx> {
    pub async fn new(
        db: DBPtr,
        chain_cfg: &ChainConfig,
        miner_cfg: &MinerConfig,
        net_cfg: &NetworkConfig,
    ) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let mut discv = Discovery::new(keypair.public(), Role::Miner, net_cfg.mdns).await?;
        discv.add_address_from_net_config(net_cfg);
        let pubsub = PubSub::new(keypair, &[PubSubTopic::TxProposal], &[])?;
        let snapshot = Snapshot::<Block, TxTrie>::load_from_db(&db, chain_cfg.state_len)?;
        let latest_block_header = snapshot.to_latest_block_header();
        let latest_tx_count = LatestTxCount::new(0);
        let worker = BlockProposalWorker::new(
            chain_cfg.clone(),
            miner_cfg.clone(),
            snapshot,
            latest_block_header,
            latest_tx_count,
            db,
        );

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
        if let Poll::Ready(blk_proposal) = self.worker.poll_block_proposal(cx) {
            self.pubsub
                .publish_block_proposal(&blk_proposal)
                .expect("Failed to publish block proposal.");
        }

        Poll::Pending
    }
}

impl<Tx: TxTrait + Serialize> NetworkBehaviourEventProcess<DiscoveryEvent> for MinerBehavior<Tx> {
    fn inject_event(&mut self, _: DiscoveryEvent) {}
}

impl<Tx: TxTrait + Serialize>
    NetworkBehaviourEventProcess<PubSubEvent<TxProposal<Tx>, BlockProposal<Block, Tx>>>
    for MinerBehavior<Tx>
{
    fn inject_event(&mut self, event: PubSubEvent<TxProposal<Tx>, BlockProposal<Block, Tx>>) {
        if let PubSubEvent::TxProposal(input) = event {
            record_event!("miner_recv_tx", "tx_id": input.tx.id());
            self.worker.add_tx_proposal(input);
        }
    }
}

#[async_trait]
impl<Tx: TxTrait + Serialize> Shutdown for MinerBehavior<Tx> {
    async fn shutdown(&mut self) -> Result<()> {
        self.worker.shutdown().await
    }
}
