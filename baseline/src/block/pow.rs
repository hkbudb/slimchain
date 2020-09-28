use crate::{
    block::{block_header_to_digest, BlockHeader, BlockTrait, BlockTxList},
    config::PoWConfig,
};
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

fn block_hash(header_hash: H256, diff: u64, nonce: Nonce) -> H256 {
    let mut hash_state = default_blake2().to_state();
    hash_state.update(header_hash.as_bytes());
    hash_state.update(diff.to_digest().as_bytes());
    hash_state.update(nonce.to_digest().as_bytes());
    let hash = hash_state.finalize();
    blake2b_hash_to_h256(hash)
}

impl Digestible for Block {
    fn to_digest(&self) -> H256 {
        block_hash(self.header.to_digest(), self.diff, self.nonce)
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
            diff: PoWConfig::get().init_diff,
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
fn nonce_is_valid(blk_hash: H256, diff: u64) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }

    let target = U256::MAX / U256::from(diff);
    debug!("mining target: {}", target);

    let hash = U256::from(blk_hash.to_fixed_bytes());
    hash <= target
}

#[tracing::instrument(skip(header, prev_blk), fields(height = header.height.0))]
pub fn create_new_block(header: BlockHeader, prev_blk: &Block) -> Block {
    debug!("Begin mining");
    let begin = Instant::now();
    let diff = compute_diff(header.time_stamp, prev_blk);
    let mut blk = Block {
        header,
        diff,
        nonce: Nonce::zero(),
    };

    let tx_list_root = blk.header.tx_list.to_digest();

    while !nonce_is_valid(
        block_hash(
            block_header_to_digest(
                blk.header.height,
                blk.header.prev_blk_hash,
                blk.header.time_stamp,
                tx_list_root,
                blk.header.state_root,
            ),
            blk.diff,
            blk.nonce,
        ),
        blk.diff,
    ) {
        blk.header.time_stamp = Utc::now();
        blk.diff = compute_diff(blk.header.time_stamp, prev_blk);
        blk.nonce += 1.into();
    }

    let mining_time = Instant::now() - begin;
    record_time!("mining", mining_time, "height": blk.header.height.0);
    info!(?mining_time, diff = blk.diff);
    blk
}

pub fn verify_consensus(blk: &Block, prev_blk: &Block) -> Result<()> {
    ensure!(
        blk.diff == compute_diff(blk.header.time_stamp, prev_blk),
        "Invalid difficult."
    );
    ensure!(nonce_is_valid(blk.to_digest(), blk.diff), "Invalid nonce");

    Ok(())
}
