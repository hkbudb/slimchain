use crate::block::{BlockHeader, BlockTrait, BlockTxList};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Nonce, H256, U256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{ensure, Result},
};
use slimchain_utils::record_time;
use std::time::Instant;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    header: BlockHeader,
    diff: u64,
    nonce: Nonce,
}

impl Digestible for Block {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.header.to_digest().as_bytes());
        hash_state.update(self.diff.to_digest().as_bytes());
        hash_state.update(self.nonce.to_digest().as_bytes());
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl BlockTrait for Block {
    fn genesis_block() -> Self {
        Self {
            header: BlockHeader {
                height: 0.into(),
                prev_blk_hash: H256::zero(),
                time_stamp: DateTime::parse_from_rfc3339("2020-08-01T00:00:00Z")
                    .expect("Failed to parse the timestamp.")
                    .with_timezone(&Utc),
                tx_list: BlockTxList::default(),
                state_root: H256::zero(),
            },
            diff: 0x10000,
            nonce: Nonce::zero(),
        }
    }

    fn block_header(&self) -> &BlockHeader {
        &self.header
    }

    fn block_header_mut(&mut self) -> &mut BlockHeader {
        &mut self.header
    }
}

// Ref:
// https://ethereum.stackexchange.com/a/1910
// https://ethereum.github.io/yellowpaper/paper.pdf
#[inline]
fn compute_diff(time_stamp: DateTime<Utc>, prev_blk: &Block) -> u64 {
    let prev_diff = prev_blk.diff as i64;
    let delta = prev_diff / 2048;
    let time_span = (time_stamp - prev_blk.header.time_stamp).num_seconds() as i64;
    let coeff = core::cmp::max(1 - time_span / 10, -99);
    (prev_diff + delta * coeff) as u64
}

#[inline]
fn nonce_is_valid(blk: &Block) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }

    let hash = U256::from(blk.to_digest().to_fixed_bytes());
    hash <= U256::MAX / blk.diff
}

#[tracing::instrument(skip(header, prev_blk), fields(height = header.height.0))]
pub fn create_new_block(header: BlockHeader, prev_blk: &Block) -> Block {
    let begin = Instant::now();
    let diff = compute_diff(header.time_stamp, prev_blk);
    let mut blk = Block {
        header,
        diff,
        nonce: Nonce::zero(),
    };

    while !nonce_is_valid(&blk) {
        blk.header.time_stamp = Utc::now();
        blk.diff = compute_diff(blk.header.time_stamp, prev_blk);
        blk.nonce += 1.into();
    }

    record_time!("mining", Instant::now() - begin, "height": blk.header.height.0);
    blk
}

pub fn verify_consensus(blk: &Block, prev_blk: &Block) -> Result<()> {
    ensure!(
        blk.diff == compute_diff(blk.header.time_stamp, prev_blk),
        "Invalid difficult."
    );
    ensure!(nonce_is_valid(blk), "Invalid nonce");

    Ok(())
}
