use serde::Deserialize;

pub use slimchain_chain::{config::MinerConfig, consensus::Consensus, role::Role};
pub use slimchain_network::config::NetworkConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Consensus method. Possible values: pow, raft.
    pub consensus: Consensus,
}
