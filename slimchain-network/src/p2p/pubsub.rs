use libp2p::{
    gossipsub::{
        Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic,
        MessageAuthenticity, MessageId, TopicHash,
    },
    identity::Keypair,
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    collections::{HashMap, HashSet},
    digest::Digestible,
    error::{anyhow, ensure, Result},
};
use std::{
    collections::VecDeque,
    task::{Context, Poll},
    time::Duration,
};

const MAX_MESSAGE_SIZE: usize = 20_000_000;
const MAX_TRANSMIT_SIZE: usize = 25_000_000;
const DUPLICATE_CACHE_TTL: Duration = Duration::from_secs(1_800);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

static TOPIC_MAP: Lazy<HashMap<TopicHash, PubSubTopic>> = Lazy::new(|| {
    let mut map = HashMap::with_capacity(2);
    for &topic in &[PubSubTopic::TxProposal, PubSubTopic::BlockProposal] {
        map.insert(topic.into_topic_hash(), topic);
    }
    map
});

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum PubSubTopic {
    TxProposal,
    BlockProposal,
}

impl PubSubTopic {
    pub fn into_topic(self) -> IdentTopic {
        match self {
            PubSubTopic::TxProposal => IdentTopic::new("tx_proposal".to_string()),
            PubSubTopic::BlockProposal => IdentTopic::new("block_proposal".to_string()),
        }
    }

    pub fn into_topic_hash(self) -> TopicHash {
        self.into_topic().hash()
    }
}

#[derive(Debug)]
pub enum PubSubEvent<TxProposal, BlockProposal> {
    TxProposal(TxProposal),
    BlockProposal(BlockProposal),
}

#[derive(NetworkBehaviour)]
#[behaviour(
    poll_method = "poll_inner",
    out_event = "PubSubEvent<TxProposal, BlockProposal>"
)]
pub struct PubSub<TxProposal, BlockProposal>
where
    TxProposal: Send + 'static,
    BlockProposal: Send + 'static,
{
    gossipsub: Gossipsub,
    #[behaviour(ignore)]
    pending_events: VecDeque<PubSubEvent<TxProposal, BlockProposal>>,
    #[behaviour(ignore)]
    sub_topics: HashSet<PubSubTopic>,
}

impl<TxProposal, BlockProposal> PubSub<TxProposal, BlockProposal>
where
    TxProposal: Send + 'static,
    BlockProposal: Send + 'static,
{
    pub fn new(
        keypair: Keypair,
        sub_topics: &[PubSubTopic],
        relay_topics: &[PubSubTopic],
    ) -> Result<Self> {
        let cfg = GossipsubConfigBuilder::default()
            .protocol_id_prefix("/slimchain/pubsub/1")
            .heartbeat_interval(HEARTBEAT_INTERVAL)
            .duplicate_cache_time(DUPLICATE_CACHE_TTL)
            .message_id_fn(|msg: &GossipsubMessage| {
                let hash = msg.data.to_digest();
                MessageId::new(hash.as_bytes())
            })
            .max_transmit_size(MAX_TRANSMIT_SIZE)
            .build()
            .map_err(|e| anyhow!("Failed to create gossipsub config. Error: {}", e))?;

        let mut gossipsub = Gossipsub::new(MessageAuthenticity::Signed(keypair), cfg)
            .map_err(|e| anyhow!("Failed to create gossipsub. Error: {}", e))?;

        for topic in sub_topics {
            gossipsub
                .subscribe(&topic.into_topic())
                .map_err(|e| anyhow!("Failed to subscribe. Error: {:?}", e))?;
        }

        for topic in relay_topics {
            gossipsub
                .subscribe(&topic.into_topic())
                .map_err(|e| anyhow!("Failed to subscribe. Error: {:?}", e))?;
        }

        Ok(Self {
            gossipsub,
            pending_events: VecDeque::new(),
            sub_topics: sub_topics.iter().copied().collect(),
        })
    }

    fn poll_inner<T>(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, PubSubEvent<TxProposal, BlockProposal>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
        }

        Poll::Pending
    }

    pub fn report_known_peers(&self) {
        println!("[PubSub] Known peers:");
        for (peer_id, topic_hashes) in self.gossipsub.all_peers() {
            println!(
                " {:?} => {:#?}",
                peer_id,
                topic_hashes
                    .iter()
                    .map(|hash| TOPIC_MAP.get(&hash))
                    .collect::<Vec<_>>()
            );
        }
    }
}

impl<TxProposal, BlockProposal> PubSub<TxProposal, BlockProposal>
where
    TxProposal: Serialize + Send + 'static,
    BlockProposal: Serialize + Send + 'static,
{
    pub fn publish_tx_proposal(&mut self, input: &TxProposal) -> Result<()> {
        let data = postcard::to_allocvec(input)?;
        ensure!(
            data.len() < MAX_MESSAGE_SIZE,
            "PubSub: data is too large. Size={}.",
            data.len()
        );
        self.gossipsub
            .publish(PubSubTopic::TxProposal.into_topic(), data)
            .map_err(|e| anyhow!("PubSub: Failed to publish tx proposal. Error: {:?}", e))?;
        Ok(())
    }

    pub fn publish_block_proposal(&mut self, input: &BlockProposal) -> Result<()> {
        let data = postcard::to_allocvec(input)?;
        ensure!(
            data.len() < MAX_MESSAGE_SIZE,
            "PubSub: data is too large. Size={}.",
            data.len()
        );
        self.gossipsub
            .publish(PubSubTopic::BlockProposal.into_topic(), data)
            .map_err(|e| anyhow!("PubSub: Failed to publish block proposal. Error: {:?}", e))?;
        Ok(())
    }
}

impl<TxProposal, BlockProposal> NetworkBehaviourEventProcess<GossipsubEvent>
    for PubSub<TxProposal, BlockProposal>
where
    TxProposal: for<'de> Deserialize<'de> + Send + 'static,
    BlockProposal: for<'de> Deserialize<'de> + Send + 'static,
{
    fn inject_event(&mut self, event: GossipsubEvent) {
        if let GossipsubEvent::Message {
            message:
                GossipsubMessage {
                    data,
                    topic: topic_hash,
                    ..
                },
            ..
        } = event
        {
            let topic = match TOPIC_MAP.get(&topic_hash) {
                Some(topic) => topic,
                None => {
                    warn!(?topic_hash, "PubSub: Unknown topic.");
                    return;
                }
            };

            if !self.sub_topics.contains(topic) {
                return;
            }

            match topic {
                PubSubTopic::TxProposal => {
                    let input = postcard::from_bytes(data.as_slice())
                        .expect("PubSub: Failed to decode message.");
                    self.pending_events
                        .push_back(PubSubEvent::TxProposal(input));
                }
                PubSubTopic::BlockProposal => {
                    let input = postcard::from_bytes(data.as_slice())
                        .expect("PubSub: Failed to decode message.");
                    self.pending_events
                        .push_back(PubSubEvent::BlockProposal(input));
                }
            }
        }
    }
}
