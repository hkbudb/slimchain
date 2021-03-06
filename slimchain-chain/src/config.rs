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
    #[serde(default = "default_max_txs")]
    pub max_txs: usize,
    /// Min number of txs in one block. It should be greater than 0.
    #[serde(default)]
    pub min_txs: usize,
    /// Max time span used in collecting txs.
    #[serde(deserialize_with = "slimchain_utils::config::deserialize_duration_from_millis")]
    pub max_block_interval: Duration,
    /// Whether to compress partial tries. Default true.
    #[serde(default = "default_compress_trie")]
    pub compress_trie: bool,
}

fn default_max_txs() -> usize {
    512
}

fn default_compress_trie() -> bool {
    true
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
