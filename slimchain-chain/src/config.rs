use crate::{conflict_check::ConflictCheck, consensus::Consensus};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use slimchain_common::error::{anyhow, Result};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Conflict check method. Possible values: ssi, occ.
    pub conflict_check: ConflictCheck,
    /// The number of blocks in the temp state.
    pub state_len: usize,
    /// Consensus method. Possible values: pow, raft.
    pub consensus: Consensus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinerConfig {
    /// Max number of txs in one block.
    #[serde(default = "usize::max_value")]
    pub max_txs: usize,
    /// Min number of txs in one block.
    #[serde(default)]
    pub min_txs: usize,
    /// Max time span between blocks in seconds.
    #[serde(deserialize_with = "slimchain_utils::config::deserialize_duration_from_secs")]
    pub max_block_interval: Duration,
}

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(default)]
pub struct PoWConfig {
    /// The initial difficulty used by PoW.
    pub init_diff: u64,
}

impl Default for PoWConfig {
    fn default() -> Self {
        Self {
            init_diff: 5_000_000,
        }
    }
}

static GLOBAL_POW_CONFIG: OnceCell<PoWConfig> = OnceCell::new();

impl PoWConfig {
    pub fn install_as_global(self) -> Result<()> {
        GLOBAL_POW_CONFIG
            .set(self)
            .map_err(|_| anyhow!("Failed to set PoWConfig."))
    }

    pub fn get() -> Self {
        GLOBAL_POW_CONFIG.get().copied().unwrap_or_default()
    }
}
