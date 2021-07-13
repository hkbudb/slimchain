use crate::p2p::config::NetworkConfig;
use futures::{channel::oneshot, prelude::*};
use futures_timer::Delay;
use libp2p::{
    identify::{Identify, IdentifyConfig, IdentifyEvent},
    identity::PublicKey,
    kad::{
        record::store::MemoryStore as KadMemoryStore, record::Key as KadKey, GetProvidersOk,
        Kademlia, KademliaConfig, KademliaEvent, QueryId as KadQueryId,
        QueryResult as KadQueryResult,
    },
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    ping::{Ping, PingConfig, PingEvent},
    swarm::{toggle::Toggle, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    Multiaddr, NetworkBehaviour, PeerId,
};
use rand::seq::IteratorRandom;
use slimchain_chain::role::Role;
use slimchain_common::{
    collections::{HashMap, HashSet},
    create_id_type_u64,
    error::{anyhow, Result},
};
use std::{
    cmp,
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::time::delay_queue::{DelayQueue, Key as DelayQueueKey};

const RETRY_WAIT_INTERVAL: Duration = Duration::from_millis(500);
const KAD_MAX_INTERVAL: Duration = Duration::from_secs(60);
const KAD_INIT_INTERVAL: Duration = Duration::from_secs(1);
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PING_TIMEOUT: Duration = Duration::from_secs(45);

create_id_type_u64!(QueryId);

#[derive(Debug)]
#[non_exhaustive]
pub enum DiscoveryEvent {
    FindPeerResult {
        query_id: QueryId,
        peer: Result<PeerId>,
    },
}

#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll_inner", out_event = "DiscoveryEvent")]
pub struct Discovery {
    kad: Kademlia<KadMemoryStore>,
    identify: Identify,
    ping: Ping,
    mdns: Toggle<Mdns>,
    #[behaviour(ignore)]
    peer_id: PeerId,
    #[behaviour(ignore)]
    peer_table: HashMap<Role, HashSet<PeerId>>,
    #[behaviour(ignore)]
    rev_peer_table: HashMap<PeerId, Role>,
    #[behaviour(ignore)]
    duration_to_next_kad: Duration,
    #[behaviour(ignore)]
    next_kad_query: Delay,
    #[behaviour(ignore)]
    pending_queries: HashMap<KadQueryId, (QueryId, Role, DelayQueueKey)>,
    #[behaviour(ignore)]
    exp_queries: DelayQueue<KadQueryId>,
    #[behaviour(ignore)]
    pending_retry_queries: DelayQueue<(QueryId, Role, Instant)>,
    #[behaviour(ignore)]
    pending_events: VecDeque<DiscoveryEvent>,
    #[behaviour(ignore)]
    pending_queries_using_ret: HashMap<QueryId, oneshot::Sender<Result<PeerId>>>,
}

impl Discovery {
    pub async fn new(pk: PublicKey, role: Role, enable_mdns: bool) -> Result<Self> {
        let peer_id = PeerId::from(pk.clone());

        let mut kad = {
            let mut config = KademliaConfig::default();
            config.set_protocol_name(&b"/slimchain/discv/kad/1"[..]);
            Kademlia::with_config(peer_id, KadMemoryStore::new(peer_id), config)
        };
        kad.start_providing(role_to_kad_key(role))
            .map_err(|e| anyhow!("Failed to announce role. Error:{:?}", e))?;

        let identify_cfg = IdentifyConfig::new("/slimchain/discv/identify/1".to_string(), pk)
            .with_agent_version(role.to_user_agent());
        let identify = Identify::new(identify_cfg);

        let ping_cfg = PingConfig::new()
            .with_interval(PING_INTERVAL)
            .with_timeout(PING_TIMEOUT)
            .with_keep_alive(true);
        let ping = Ping::new(ping_cfg);

        let mdns = if enable_mdns {
            let mdns_cfg = MdnsConfig::default();
            match Mdns::new(mdns_cfg).await {
                Ok(mdns) => Some(mdns),
                Err(e) => {
                    error!("Failed to enable mdns. Error: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            kad,
            identify,
            ping,
            mdns: mdns.into(),
            peer_id,
            peer_table: HashMap::new(),
            rev_peer_table: HashMap::new(),
            duration_to_next_kad: KAD_INIT_INTERVAL,
            next_kad_query: Delay::new(Duration::from_secs(0)),
            pending_queries: HashMap::new(),
            exp_queries: DelayQueue::new(),
            pending_retry_queries: DelayQueue::new(),
            pending_events: VecDeque::new(),
            pending_queries_using_ret: HashMap::new(),
        })
    }

    pub fn add_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        if peer_id != self.peer_id {
            self.kad.add_address(&peer_id, address);
        }
    }

    pub fn add_address_from_net_config(&mut self, cfg: &NetworkConfig) {
        for peer in cfg.peers.iter() {
            self.add_address(peer.peer_id, peer.address.clone());
        }
    }

    pub fn report_known_peers(&self) {
        println!("[Discovery] Known peers:");
        for (role, list) in &self.peer_table {
            println!(" {} => {:#?}", role, list);
        }
    }

    pub fn known_peers(&self, role: &Role) -> HashSet<PeerId> {
        self.peer_table.get(role).cloned().unwrap_or_default()
    }

    pub fn known_peer_num(&self, role: &Role) -> usize {
        self.peer_table.get(role).map_or(0, |list| list.len())
    }

    pub fn random_known_peer(&self, role: &Role) -> Option<PeerId> {
        if let Some(list) = self.peer_table.get(role) {
            let mut rng = rand::thread_rng();
            list.iter().choose(&mut rng).copied()
        } else {
            None
        }
    }

    pub fn random_known_peers(&self, role: &Role, amount: usize) -> Vec<PeerId> {
        if let Some(list) = self.peer_table.get(role) {
            let mut rng = rand::thread_rng();
            list.iter()
                .choose_multiple(&mut rng, amount)
                .into_iter()
                .copied()
                .collect()
        } else {
            vec![]
        }
    }

    pub fn find_random_peer(&mut self, role: Role, timeout: Duration) -> QueryId {
        let query_id = QueryId::next_id();
        if let Some(peer) = self.random_known_peer(&role) {
            self.pending_events
                .push_back(DiscoveryEvent::FindPeerResult {
                    query_id,
                    peer: Ok(peer),
                });
        } else {
            let kad_query_id = self.kad.get_providers(role_to_kad_key(role));
            let delay = self.exp_queries.insert(kad_query_id, timeout);
            self.pending_queries
                .insert(kad_query_id, (query_id, role, delay));
        }
        query_id
    }

    pub fn find_random_peer_with_ret(
        &mut self,
        role: Role,
        timeout: Duration,
        ret: oneshot::Sender<Result<PeerId>>,
    ) {
        let id = self.find_random_peer(role, timeout);
        self.pending_queries_using_ret.insert(id, ret);
    }

    fn peer_table_add_node(&mut self, peer_id: PeerId, role: Role) {
        use slimchain_common::collections::hash_map::Entry;

        match self.rev_peer_table.entry(peer_id) {
            Entry::Occupied(mut o) => {
                trace!("Refresh node {} with role {}", peer_id, role);
                let old_role = *o.get();
                if old_role != role {
                    *o.get_mut() = role;
                    self.peer_table
                        .get_mut(&old_role)
                        .map(|list| list.remove(&peer_id));
                    self.peer_table.entry(role).or_default().insert(peer_id);
                }
            }
            Entry::Vacant(v) => {
                trace!("Add node {} with role {}", peer_id, role);
                v.insert(role);
                self.peer_table.entry(role).or_default().insert(peer_id);
            }
        }
    }

    fn peer_table_remove_node(&mut self, peer_id: PeerId) {
        let role = match self.rev_peer_table.remove(&peer_id) {
            Some(role) => role,
            None => {
                return;
            }
        };
        trace!("Remove node {} with role {}", peer_id, role);
        self.peer_table
            .get_mut(&role)
            .map(|list| list.remove(&peer_id));
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, DiscoveryEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            let DiscoveryEvent::FindPeerResult { query_id, peer } = event;
            if let Some(tx) = self.pending_queries_using_ret.remove(&query_id) {
                tx.send(peer).ok();
            } else {
                return Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                    DiscoveryEvent::FindPeerResult { query_id, peer },
                ));
            }
        }

        while Pin::new(&mut self.next_kad_query).poll(cx).is_ready() {
            self.kad.get_closest_peers(PeerId::random());

            self.next_kad_query = Delay::new(self.duration_to_next_kad);
            self.duration_to_next_kad = cmp::min(self.duration_to_next_kad * 2, KAD_MAX_INTERVAL);
        }

        while let Poll::Ready(Some(Ok(kad_query_id))) = self.exp_queries.poll_expired(cx) {
            if let Some((query_id, role, _)) = self.pending_queries.remove(kad_query_id.get_ref()) {
                if let Some(peer) = self.random_known_peer(&role) {
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                        DiscoveryEvent::FindPeerResult {
                            query_id,
                            peer: Ok(peer),
                        },
                    ));
                } else {
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                        DiscoveryEvent::FindPeerResult {
                            query_id,
                            peer: Err(anyhow!("Timeout when find peer.")),
                        },
                    ));
                }
            }
        }

        while let Poll::Ready(Some(Ok(query))) = self.pending_retry_queries.poll_expired(cx) {
            let (query_id, role, deadline) = query.into_inner();

            if let Some(peer) = self.random_known_peer(&role) {
                self.pending_events
                    .push_back(DiscoveryEvent::FindPeerResult {
                        query_id,
                        peer: Ok(peer),
                    });
            } else if deadline <= Instant::now() {
                self.pending_events
                    .push_back(DiscoveryEvent::FindPeerResult {
                        query_id,
                        peer: Err(anyhow!("Timeout when find peer.")),
                    });
            } else {
                let kad_query_id = self.kad.get_providers(role_to_kad_key(role));
                let delay = self.exp_queries.insert_at(kad_query_id, deadline);
                self.pending_queries
                    .insert(kad_query_id, (query_id, role, delay));
            }
        }

        Poll::Pending
    }
}

impl NetworkBehaviourEventProcess<IdentifyEvent> for Discovery {
    fn inject_event(&mut self, event: IdentifyEvent) {
        if let IdentifyEvent::Received { peer_id, info, .. } = event {
            let role = match Role::from_user_agent(&info.agent_version) {
                Ok(role) => role,
                Err(e) => {
                    error!(
                        "Failed to parse user-agent ({}) from {}. Error: {:?}",
                        info.agent_version, peer_id, e
                    );
                    return;
                }
            };
            self.peer_table_add_node(peer_id, role);

            for addr in info.listen_addrs {
                self.add_address(peer_id, addr);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for Discovery {
    fn inject_event(&mut self, event: PingEvent) {
        let PingEvent { peer, result } = event;
        if result.is_err() {
            self.peer_table_remove_node(peer);
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for Discovery {
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, address) in list {
                trace!("Discovered peer {} at {} from mdns.", peer_id, address);
                self.add_address(peer_id, address);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for Discovery {
    fn inject_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::OutboundQueryCompleted {
                id,
                result: KadQueryResult::GetProviders(result),
                ..
            } => {
                let (query_id, role, delay) = match self.pending_queries.remove(&id) {
                    Some(entry) => entry,
                    None => return,
                };
                let deadline = self.exp_queries.remove(&delay).deadline();

                if let Ok(GetProvidersOk { providers, .. }) = result {
                    for peer_id in providers {
                        self.peer_table_add_node(peer_id, role);
                    }
                }

                if let Some(peer) = self.random_known_peer(&role) {
                    self.pending_events
                        .push_back(DiscoveryEvent::FindPeerResult {
                            query_id,
                            peer: Ok(peer),
                        });
                } else if deadline <= Instant::now() {
                    self.pending_events
                        .push_back(DiscoveryEvent::FindPeerResult {
                            query_id,
                            peer: Err(anyhow!("Timeout when find peer.")),
                        });
                } else {
                    self.pending_retry_queries
                        .insert((query_id, role, deadline), RETRY_WAIT_INTERVAL);
                }
            }
            KademliaEvent::OutboundQueryCompleted {
                result: KadQueryResult::StartProviding(Err(error)),
                ..
            } => {
                error!("Failed to announce role. Error: {:?}", error);
            }
            _ => {}
        }
    }
}

fn role_to_kad_key(role: Role) -> KadKey {
    KadKey::new(&format!("{}", role).as_bytes())
}

#[cfg(test)]
mod tests;
