use crate::{
    block::{block_header_to_digest, BlockHeader, BlockTrait, BlockTxList},
    config::PoWConfig,
};
use chrono::{DateTime, Utc};
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Nonce, H256, U256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{ensure, Error, Result},
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
    compute_diff_inner(time_stamp, prev_blk.diff, prev_blk.header.time_stamp)
}

#[inline]
fn compute_diff_inner(time_stamp: DateTime<Utc>, prev_diff: u64, prev_ts: DateTime<Utc>) -> u64 {
    let prev_diff = prev_diff as i64;
    let delta = prev_diff / 2048;
    let time_span = (time_stamp - prev_ts).num_seconds() as i64;
    let coeff = core::cmp::max(1 - time_span / 10, -99);
    (prev_diff + delta * coeff) as u64
}

#[inline]
fn nonce_is_valid(blk_hash: H256, diff: u64) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }

    let target = U256::MAX / U256::from(diff);
    let hash = U256::from(blk_hash.to_fixed_bytes());
    hash <= target
}

#[tracing::instrument(skip(header, prev_blk), fields(height = header.height.0))]
pub fn create_new_block(
    header: BlockHeader,
    prev_blk: &Block,
) -> impl Future<Output = Result<Block>> {
    debug!("Begin mining");
    let begin = Instant::now();
    let prev_diff = prev_blk.diff;
    let prev_ts = prev_blk.header.time_stamp;
    let diff = compute_diff_inner(header.time_stamp, prev_diff, prev_ts);

    tokio::task::spawn_blocking(move || {
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
            blk.header.set_ts(Utc::now());
            blk.diff = compute_diff_inner(blk.header.time_stamp, prev_diff, prev_ts);
            blk.nonce += 1.into();
        }

        let mining_time = Instant::now() - begin;
        record_time!("mining", mining_time, "height": blk.header.height.0);
        info!(?mining_time, diff = blk.diff);
        blk
    })
    .map_err(Error::msg)
}

pub fn verify_consensus(blk: &Block, prev_blk: &Block) -> Result<()> {
    ensure!(
        blk.diff == compute_diff(blk.header.time_stamp, prev_blk),
        "Invalid difficult."
    );
    ensure!(nonce_is_valid(blk.to_digest(), blk.diff), "Invalid nonce");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slimchain_utils::config::Config;

    #[tokio::test]
    #[ignore]
    async fn test_pow() {
        let pow_cfg = Config::load_test()
            .and_then(|cfg| cfg.get::<PoWConfig>("pow"))
            .unwrap_or_default();
        pow_cfg.install_as_global().ok();

        let mut blk = Block::genesis_block();
        blk.header.tx_list = std::iter::repeat_with(H256::zero).take(100).collect();

        for _ in 0..30 {
            let mut header = blk.header.clone();
            header.height = header.height.next_height();
            header.set_ts(Utc::now());
            let new_blk = create_new_block(header, &blk).await.unwrap();
            println!("diff = {}", new_blk.diff);
            println!("time = {}", new_blk.time_stamp() - blk.time_stamp());
            println!("nonce = {}", new_blk.nonce);
            println!("target = {}", U256::MAX / U256::from(new_blk.diff));
            println!("---------------------");
            blk = new_blk;
        }
    }
}
