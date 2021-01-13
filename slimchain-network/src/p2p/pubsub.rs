use libp2p::{
    gossipsub::{
        error::PublishError, Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage,
        IdentTopic, MessageAuthenticity, TopicHash,
    },
    identity::Keypair,
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour, PeerId,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::H256,
    collections::{HashMap, HashSet},
    digest::Digestible,
    error::{anyhow, ensure, Result},
};
use std::{
    cmp,
    collections::VecDeque,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::DelayQueue;

const MAX_MESSAGE_SIZE: usize = 45_000_000;
const MAX_TRANSMIT_SIZE: usize = 50_000_000;
const DUPLICATE_CACHE_TTL: Duration = Duration::from_secs(1_800);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const CHECK_EXPLICIT_PEERS_TICKS: u64 = 5;
const PUB_MAX_RETRIES: usize = 5;
const PUB_INIT_RETRY_DELAY: Duration = Duration::from_millis(500);
const PUB_MAX_RETRY_DELAY: Duration = Duration::from_secs(2);

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
    #[behaviour(ignore)]
    retry_messages: DelayQueue<(PubSubTopic, Vec<u8>, usize, Duration)>,
    #[behaviour(ignore)]
    dup_cache: HashSet<H256>,
    #[behaviour(ignore)]
    dup_cache_ttl: DelayQueue<H256>,
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
            .check_explicit_peers_ticks(CHECK_EXPLICIT_PEERS_TICKS)
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
            retry_messages: DelayQueue::new(),
            dup_cache: HashSet::new(),
            dup_cache_ttl: DelayQueue::new(),
        })
    }

    fn publish_message(
        &mut self,
        topic: PubSubTopic,
        data: Vec<u8>,
        retries: usize,
        retry_delay: Duration,
    ) {
        match self.gossipsub.publish(topic.into_topic(), data.clone()) {
            Ok(_) => return,
            Err(PublishError::InsufficientPeers) => {}
            Err(e) => {
                panic!("PubSub: Failed to publish message. Error: {:?}", e);
            }
        }

        if retries == 0 {
            self.report_known_peers();
            panic!("PubSub: Failed to publish message. Reaching max retries.");
        }

        self.retry_messages.insert(
            (
                topic,
                data,
                retries - 1,
                cmp::min(retry_delay * 2, PUB_MAX_RETRY_DELAY),
            ),
            retry_delay,
        );
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, PubSubEvent<TxProposal, BlockProposal>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
        }

        while let Poll::Ready(Some(Ok(message))) = self.retry_messages.poll_expired(cx) {
            let (topic, data, retries, delay) = message.into_inner();
            trace!(retries, ?delay, "PubSub: retry to publish the message.");
            self.publish_message(topic, data, retries, delay);
        }

        while let Poll::Ready(Some(Ok(key))) = self.dup_cache_ttl.poll_expired(cx) {
            self.dup_cache.remove(key.get_ref());
        }

        Poll::Pending
    }

    pub fn add_explicit_peer(&mut self, peer: PeerId) {
        self.gossipsub.add_explicit_peer(&peer);
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
        self.publish_message(
            PubSubTopic::TxProposal,
            data,
            PUB_MAX_RETRIES,
            PUB_INIT_RETRY_DELAY,
        );
        Ok(())
    }

    pub fn publish_block_proposal(&mut self, input: &BlockProposal) -> Result<()> {
        let data = postcard::to_allocvec(input)?;
        ensure!(
            data.len() < MAX_MESSAGE_SIZE,
            "PubSub: data is too large. Size={}.",
            data.len()
        );
        self.publish_message(
            PubSubTopic::BlockProposal,
            data,
            PUB_MAX_RETRIES,
            PUB_INIT_RETRY_DELAY,
        );
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

            let data_hash = data.to_digest();

            if !self.dup_cache.insert(data_hash) {
                return;
            }
            self.dup_cache_ttl.insert(data_hash, DUPLICATE_CACHE_TTL);

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
