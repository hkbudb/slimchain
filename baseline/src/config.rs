use serde::Deserialize;

pub use slimchain_chain::{
    config::{MinerConfig, PoWConfig},
    consensus::Consensus,
    role::Role,
};
pub use slimchain_network::p2p::config::NetworkConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Consensus method. Possible values: pow, raft.
    pub consensus: Consensus,
}
