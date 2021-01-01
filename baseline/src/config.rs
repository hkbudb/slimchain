use serde::Deserialize;
use slimchain_chain::consensus::Consensus;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Consensus method. Possible values: pow, raft.
    pub consensus: Consensus,
}
