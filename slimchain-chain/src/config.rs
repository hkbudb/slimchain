use crate::conflict_check::ConflictCheck;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Conflict check method. Possible values: ssi, occ.
    pub conflict_check: ConflictCheck,
    /// The number of blocks in the temp state.
    pub state_len: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinerConfig {
    /// Max number of txs in one block.
    pub max_txs: usize,
    /// Min number of txs in one block.
    #[serde(default)]
    pub min_txs: usize,
    /// Max time span between blocks in seconds.
    #[serde(deserialize_with = "slimchain_utils::config::deserialize_duration_from_secs")]
    pub max_block_interval: Duration,
}
