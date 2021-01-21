use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use slimchain_chain::role::Role;
use slimchain_common::{
    collections::HashMap,
    error::{anyhow, Result},
    utils::derive_more,
};
use std::sync::Arc;

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct PeerId(pub u64);

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    /// The peer id of this node
    pub peer_id: PeerId,

    /// Listen address for HTTP server
    #[serde(default = "default_http_listen")]
    pub http_listen: String,

    /// Known peers
    #[serde(default = "Vec::new")]
    pub peers: Vec<PeerConfig>,
}

fn default_http_listen() -> String {
    "127.0.0.1:8000".into()
}

impl NetworkConfig {
    pub fn to_route_table(&self) -> NetworkRouteTable {
        let mut peer_table = HashMap::new();
        for peer in &self.peers {
            peer_table.insert(peer.peer_id, peer.address.clone());
        }

        let mut role_table = HashMap::new();
        for peer in &self.peers {
            role_table
                .entry(peer.role)
                .or_insert_with(Vec::new)
                .push(peer.peer_id);
        }

        NetworkRouteTable {
            peer_id: self.peer_id,
            peer_table,
            role_table,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkRouteTable {
    peer_id: PeerId,
    peer_table: HashMap<PeerId, String>,
    role_table: HashMap<Role, Vec<PeerId>>,
}

impl NetworkRouteTable {
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn all_client_peer_ids(&self) -> std::collections::HashSet<async_raft::NodeId> {
        if let Some(peers) = self.role_table.get(&Role::Client) {
            peers.iter().map(|id| id.0).collect()
        } else {
            Default::default()
        }
    }

    pub fn peer_table(&self) -> &HashMap<PeerId, String> {
        &self.peer_table
    }

    pub fn role_table(&self) -> &HashMap<Role, Vec<PeerId>> {
        &self.role_table
    }

    pub fn peer_address(&self, peer_id: PeerId) -> Result<&String> {
        self.peer_table
            .get(&peer_id)
            .ok_or_else(|| anyhow!("Failed to get peer address. PeerId: {}.", peer_id))
    }

    pub fn random_peer(&self, role: &Role) -> Option<PeerId> {
        match self.role_table.get(role) {
            Some(list) => {
                let mut rng = rand::thread_rng();
                list.iter().choose(&mut rng).copied()
            }
            None => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerConfig {
    pub peer_id: PeerId,
    pub address: String,
    #[serde(flatten)]
    pub role: Role,
}

// https://docs.rs/async-raft/0.6.0-alpha.1/async_raft/config/struct.Config.html
#[derive(Debug, Clone, Deserialize)]
pub struct RaftConfig {
    /// The minimum election timeout in milliseconds.
    pub election_timeout_min: Option<u64>,
    /// The maximum election timeout in milliseconds.
    pub election_timeout_max: Option<u64>,
    /// The heartbeat interval in milliseconds at which leaders will send heartbeats to followers.
    ///
    /// Defaults to 50 milliseconds.
    ///
    /// **NOTE WELL:** it is very important that this value be greater than the amount if time
    /// it will take on average for heartbeat frames to be sent between nodes. No data processing
    /// is performed for heartbeats, so the main item of concern here is network latency. This
    /// value is also used as the default timeout for sending heartbeats.
    pub heartbeat_interval: Option<u64>,
    /// The maximum number of entries per payload allowed to be transmitted during replication.
    ///
    /// When configuring this value, it is important to note that setting this value too low could
    /// cause sub-optimal performance. This will primarily impact the speed at which slow nodes,
    /// nodes which have been offline, or nodes which are new to the cluster, are brought
    /// up-to-speed. If this is too low, it will take longer for the nodes to be brought up to
    /// consistency with the rest of the cluster.
    pub max_payload_entries: Option<u64>,
    /// The distance behind in log replication a follower must fall before it is considered "lagging".
    ///
    /// This configuration parameter controls replication streams from the leader to followers in
    /// the cluster. Once a replication stream is considered lagging, it will stop buffering
    /// entries being replicated, and instead will fetch entries directly from the log until it is
    /// up-to-speed, at which time it will transition out of "lagging" state back into "line-rate" state.
    pub replication_lag_threshold: Option<u64>,
    /// The snapshot policy to use for a Raft node.
    /// A snapshot will be generated once the log has grown the specified number of logs since the last snapshot.
    pub snapshot_policy_logs_since_last: Option<u64>,
    /// The maximum snapshot chunk size allowed when transmitting snapshots (in bytes).
    ///
    /// Defaults to 3Mib.
    pub snapshot_max_chunk_size: Option<u64>,
    /// How to broadcast the block to storage node
    #[serde(default)]
    pub async_broadcast_storage: bool,
}

impl RaftConfig {
    pub fn to_raft_config(&self) -> Result<Arc<async_raft::Config>> {
        let mut cfg_builder = async_raft::Config::build("slimchain".into());

        if let Some(election_timeout_min) = self.election_timeout_min {
            cfg_builder = cfg_builder.election_timeout_min(election_timeout_min);
        }

        if let Some(election_timeout_max) = self.election_timeout_max {
            cfg_builder = cfg_builder.election_timeout_max(election_timeout_max);
        }

        if let Some(heartbeat_interval) = self.heartbeat_interval {
            cfg_builder = cfg_builder.heartbeat_interval(heartbeat_interval);
        }

        if let Some(max_payload_entries) = self.max_payload_entries {
            cfg_builder = cfg_builder.max_payload_entries(max_payload_entries);
        }

        if let Some(replication_lag_threshold) = self.replication_lag_threshold {
            cfg_builder = cfg_builder.replication_lag_threshold(replication_lag_threshold);
        }

        if let Some(snapshot_policy_logs_since_last) = self.snapshot_policy_logs_since_last {
            cfg_builder = cfg_builder.snapshot_policy(async_raft::SnapshotPolicy::LogsSinceLast(
                snapshot_policy_logs_since_last,
            ));
        }

        if let Some(snapshot_max_chunk_size) = self.snapshot_max_chunk_size {
            cfg_builder = cfg_builder.snapshot_max_chunk_size(snapshot_max_chunk_size);
        }

        Ok(Arc::new(cfg_builder.validate()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_peer_config() {
        use slimchain_common::basic::ShardId;
        use slimchain_utils::{config::Config, toml};

        let input = toml::toml! {
            [peer]
            peer_id = 1
            address = "127.0.0.1:8000"
        };
        let peer: PeerConfig = Config::from_toml(input).get("peer").unwrap();
        assert_eq!(1, peer.peer_id.0);
        assert_eq!("127.0.0.1:8000", &peer.address);
        assert_eq!(Role::Client, peer.role);

        let input = toml::toml! {
            [peer]
            peer_id = 1
            address = "127.0.0.1:8000"
            role = "client"
        };
        let peer: PeerConfig = Config::from_toml(input).get("peer").unwrap();
        assert_eq!(1, peer.peer_id.0);
        assert_eq!("127.0.0.1:8000", &peer.address);
        assert_eq!(Role::Client, peer.role);

        let input = toml::toml! {
            [peer]
            peer_id = 1
            address = "127.0.0.1:8000"
            role = "storage"
        };
        let peer: PeerConfig = Config::from_toml(input).get("peer").unwrap();
        assert_eq!(1, peer.peer_id.0);
        assert_eq!("127.0.0.1:8000", &peer.address);
        assert_eq!(Role::Storage(ShardId::default()), peer.role);

        let input = toml::toml! {
            [peer]
            peer_id = 1
            address = "127.0.0.1:8000"
            role = "storage"
            shard_id = 1
            shard_total = 2
        };
        let peer: PeerConfig = Config::from_toml(input).get("peer").unwrap();
        assert_eq!(1, peer.peer_id.0);
        assert_eq!("127.0.0.1:8000", &peer.address);
        assert_eq!(Role::Storage(ShardId::new(1, 2)), peer.role);
    }
}
