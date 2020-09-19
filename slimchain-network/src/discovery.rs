use crate::config::NetworkConfig;
use futures::prelude::*;
use libp2p::{
    identify::{Identify, IdentifyEvent},
    identity::PublicKey,
    kad::{
        record::store::MemoryStore as KadMemoryStore, record::Key as KadKey, GetProvidersOk,
        Kademlia, KademliaConfig, KademliaEvent, QueryId as KadQueryId,
        QueryResult as KadQueryResult,
    },
    mdns::{Mdns, MdnsEvent},
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
};
use tokio::time::{
    delay_for, delay_queue::Key as DelayQueueKey, Delay, DelayQueue, Duration, Instant,
};

const PEER_ENTRY_TTL_SECS: u64 = 600;

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
    mdns: Toggle<Mdns>,
    #[behaviour(ignore)]
    peer_table: HashMap<Role, HashSet<PeerId>>,
    #[behaviour(ignore)]
    rev_peer_table: HashMap<PeerId, (Role, DelayQueueKey)>,
    #[behaviour(ignore)]
    exp_peers: DelayQueue<PeerId>,
    #[behaviour(ignore)]
    duration_to_next_kad: Duration,
    #[behaviour(ignore)]
    next_kad_query: Delay,
    #[behaviour(ignore)]
    pending_queries: HashMap<KadQueryId, (QueryId, Role, DelayQueueKey)>,
    #[behaviour(ignore)]
    exp_queries: DelayQueue<KadQueryId>,
    #[behaviour(ignore)]
    pending_events: VecDeque<DiscoveryEvent>,
}

impl Discovery {
    pub fn new(pk: PublicKey, role: Role, enable_mdns: bool) -> Result<Self> {
        let peer_id = PeerId::from(pk.clone());

        let mut kad = {
            let mut config = KademliaConfig::default();
            config.set_protocol_name(&b"/slimchain/discv/kad/1"[..]);
            Kademlia::with_config(peer_id.clone(), KadMemoryStore::new(peer_id), config)
        };
        kad.start_providing(role_to_kad_key(role))
            .map_err(|e| anyhow!("Failed to announce role. Error:{:?}", e))?;

        let identify = Identify::new(
            "/slimchain/discv/identify/1".to_string(),
            role.to_user_agent(),
            pk,
        );

        let mdns = if enable_mdns {
            Some(Mdns::new()?)
        } else {
            None
        };

        Ok(Self {
            kad,
            identify,
            mdns: mdns.into(),
            peer_table: HashMap::new(),
            rev_peer_table: HashMap::new(),
            exp_peers: DelayQueue::new(),
            duration_to_next_kad: Duration::from_secs(1),
            next_kad_query: delay_for(Duration::from_secs(0)),
            pending_queries: HashMap::new(),
            exp_queries: DelayQueue::new(),
            pending_events: VecDeque::new(),
        })
    }

    pub fn add_address(&mut self, peer_id: &PeerId, address: Multiaddr) {
        self.kad.add_address(peer_id, address);
    }

    pub fn add_address_from_net_config(&mut self, cfg: &NetworkConfig) {
        if let Some(peers) = cfg.peers.as_ref() {
            for peer in peers {
                self.add_address(&peer.peer_id, peer.address.clone())
            }
        }
    }

    pub fn report_known_peers(&self) {
        println!("Known peers:");
        for (role, list) in &self.peer_table {
            println!(" {} => {:?}", role, list);
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
            let mut rng = &mut rand::thread_rng();
            list.iter().choose(&mut rng).cloned()
        } else {
            None
        }
    }

    pub fn random_known_peers(&self, role: &Role, amount: usize) -> Vec<PeerId> {
        if let Some(list) = self.peer_table.get(role) {
            let mut rng = &mut rand::thread_rng();
            list.iter()
                .choose_multiple(&mut rng, amount)
                .into_iter()
                .cloned()
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

    fn peer_table_add_node(&mut self, peer_id: PeerId, role: Role) {
        use slimchain_common::collections::hash_map::Entry;

        match self.rev_peer_table.entry(peer_id.clone()) {
            Entry::Occupied(o) => {
                trace!("Refresh node {} with role {}", peer_id, role);
                let (role2, delay) = o.get();
                debug_assert_eq!(&role, role2);
                self.exp_peers
                    .reset(delay, Duration::from_secs(PEER_ENTRY_TTL_SECS));
            }
            Entry::Vacant(v) => {
                trace!("Add node {} with role {}", peer_id, role);
                let delay = self
                    .exp_peers
                    .insert(peer_id.clone(), Duration::from_secs(PEER_ENTRY_TTL_SECS));
                v.insert((role, delay));
                self.peer_table.entry(role).or_default().insert(peer_id);
            }
        }
    }

    fn peer_table_remove_expired_node(&mut self, peer_id: &PeerId) {
        let (role, _delay) = match self.rev_peer_table.remove(peer_id) {
            Some(entry) => entry,
            None => {
                return;
            }
        };
        trace!("Remove node {} with role {}", peer_id, role);
        self.peer_table
            .get_mut(&role)
            .map(|list| list.remove(peer_id));
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, DiscoveryEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
        }

        while let Poll::Ready(Some(Ok(peer_id))) = self.exp_peers.poll_expired(cx) {
            self.peer_table_remove_expired_node(peer_id.get_ref());
        }

        while let Poll::Ready(_) = Pin::new(&mut self.next_kad_query).poll(cx) {
            self.kad.get_closest_peers(PeerId::random());

            self.next_kad_query = delay_for(self.duration_to_next_kad);
            self.duration_to_next_kad =
                cmp::min(self.duration_to_next_kad * 2, Duration::from_secs(60));
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
            self.peer_table_add_node(peer_id.clone(), role);

            for addr in info.listen_addrs {
                self.kad.add_address(&peer_id, addr);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for Discovery {
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, address) in list {
                trace!("Discovered peer {} at {} from mdns.", peer_id, address);
                self.add_address(&peer_id, address);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for Discovery {
    fn inject_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::QueryResult {
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
                    let kad_query_id = self.kad.get_providers(role_to_kad_key(role));
                    let delay = self.exp_queries.insert_at(kad_query_id, deadline);
                    self.pending_queries
                        .insert(kad_query_id, (query_id, role, delay));
                }
            }
            KademliaEvent::QueryResult {
                result: KadQueryResult::StartProviding(result),
                ..
            } => {
                if let Err(error) = result {
                    error!("Failed to announce role. Error: {:?}", error);
                }
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