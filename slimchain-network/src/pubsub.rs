use libp2p::{
    gossipsub::{
        error::PublishError, Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage,
        MessageAuthenticity, MessageId, Topic, TopicHash,
    },
    identity::Keypair,
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    collections::HashMap,
    digest::Digestible,
    error::{ensure, Result},
};
use std::{
    collections::VecDeque,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::DelayQueue;

const MAX_MESSAGE_SIZE: usize = 50_000_000;
const DUPLICATE_CACHE_TTL: Duration = Duration::from_secs(300);
const PUBLISH_TTL: usize = 5;
const PUBLISH_RETRY_INIT_WAIT: Duration = Duration::from_millis(500);

static TOPIC_MAP: Lazy<HashMap<TopicHash, PubSubTopic>> = Lazy::new(|| {
    let mut map = HashMap::with_capacity(2);
    for &topic in &[PubSubTopic::TxProposal, PubSubTopic::BlockProposal] {
        map.insert(topic.into_topic_hash(), topic);
    }
    map
});

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PubSubTopic {
    TxProposal,
    BlockProposal,
}

impl PubSubTopic {
    pub fn into_topic(self) -> Topic {
        match self {
            PubSubTopic::TxProposal => Topic::new("tx_proposal".to_string()),
            PubSubTopic::BlockProposal => Topic::new("block_proposal".to_string()),
        }
    }

    pub fn into_topic_hash(self) -> TopicHash {
        self.into_topic().no_hash()
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
    retry_queue: DelayQueue<(Topic, Vec<u8>, usize, Duration)>,
}

impl<TxProposal, BlockProposal> PubSub<TxProposal, BlockProposal>
where
    TxProposal: Send + 'static,
    BlockProposal: Send + 'static,
{
    pub fn new(keypair: Keypair, sub_topics: &[PubSubTopic]) -> Self {
        let cfg = GossipsubConfigBuilder::default()
            .protocol_id(&b"/slimchain/pubsub/1"[..])
            .duplicate_cache_time(DUPLICATE_CACHE_TTL)
            .message_id_fn(|msg: &GossipsubMessage| {
                let hash = msg.data.to_digest();
                MessageId::new(hash.as_bytes())
            })
            .max_transmit_size(MAX_MESSAGE_SIZE)
            .build();
        let mut gossipsub = Gossipsub::new(MessageAuthenticity::Signed(keypair), cfg);
        for topic in sub_topics {
            gossipsub.subscribe(topic.into_topic());
        }

        Self {
            gossipsub,
            pending_events: VecDeque::new(),
            retry_queue: DelayQueue::new(),
        }
    }

    fn try_publish(
        &mut self,
        topic: Topic,
        data: Vec<u8>,
        ttl: usize,
        wait: Duration,
        warning: bool,
    ) {
        match self.gossipsub.publish(&topic, data.clone()) {
            Ok(_) => return,
            Err(PublishError::InsufficientPeers) => {}
            Err(e) => {
                error!("PubSub: Failed to publish message. Error: {:?}", e);
                return;
            }
        }

        if ttl == 0 {
            error!(
                ?topic,
                "PubSub: Failed to publish message due to insufficient peers. Too many retries."
            );

            return;
        }

        if warning {
            warn!(
                ?topic,
                "PubSub: Failed to publish message due to insufficient peers. Retry."
            );
        }

        self.retry_queue
            .insert((topic, data, ttl - 1, wait * 2), wait);
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, PubSubEvent<TxProposal, BlockProposal>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
        }

        while let Poll::Ready(Some(Ok(entry))) = self.retry_queue.poll_expired(cx) {
            let (topic, data, ttl, wait) = entry.into_inner();
            self.try_publish(topic, data, ttl, wait, false);
        }

        Poll::Pending
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
        self.try_publish(
            PubSubTopic::TxProposal.into_topic(),
            data,
            PUBLISH_TTL,
            PUBLISH_RETRY_INIT_WAIT,
            true,
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
        self.try_publish(
            PubSubTopic::BlockProposal.into_topic(),
            data,
            PUBLISH_TTL,
            PUBLISH_RETRY_INIT_WAIT,
            true,
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
        if let GossipsubEvent::Message(_, _, GossipsubMessage { data, topics, .. }) = event {
            let topic = match TOPIC_MAP.get(&topics[0]) {
                Some(topic) => topic,
                None => {
                    warn!(?topics, "PubSub: Unknown topic.");
                    return;
                }
            };
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
