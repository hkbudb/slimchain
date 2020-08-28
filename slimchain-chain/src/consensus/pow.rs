use crate::block::{BlockHeader, BlockTrait, BlockTxList};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Nonce, H256, U256, U512},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{ensure, Result},
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    header: BlockHeader,
    tx_list: BlockTxList,
    diff: u64,
    nonce: Nonce,
}

impl Digestible for Block {
    fn to_digest(&self) -> H256 {
        debug_assert_eq!(self.header.tx_root, self.tx_list.to_digest());

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
                tx_root: H256::zero(),
                state_root: H256::zero(),
            },
            tx_list: BlockTxList::default(),
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

    fn tx_list(&self) -> &BlockTxList {
        &self.tx_list
    }

    fn tx_list_mut(&mut self) -> &mut BlockTxList {
        &mut self.tx_list
    }
}

// Ref:
// https://ethereum.stackexchange.com/a/1910
// https://ethereum.github.io/yellowpaper/paper.pdf
fn compute_diff(time_stamp: DateTime<Utc>, prev_blk: &Block) -> u64 {
    let prev_diff = prev_blk.diff as i64;
    let delta = prev_diff / 2048;
    let time_span = (time_stamp - prev_blk.header.time_stamp).num_seconds() as i64;
    let coeff = core::cmp::max(1 - time_span / 9, -99);
    (prev_diff + delta * coeff) as u64
}

fn diff_to_h256(diff: u64) -> H256 {
    let num = (U512::from(U256::MAX) + U512::from(1)) / diff;
    let mut bytes = [0u8; 64];
    num.to_big_endian(&mut bytes);
    H256::from_slice(&bytes[32..])
}

pub fn create_new_block(header: BlockHeader, tx_list: BlockTxList, prev_blk: &Block) -> Block {
    let diff = compute_diff(header.time_stamp, prev_blk);
    let mut blk = Block {
        header,
        tx_list,
        diff,
        nonce: 0.into(),
    };

    while blk.to_digest() > diff_to_h256(blk.diff) {
        blk.header.time_stamp = Utc::now();
        blk.diff = compute_diff(blk.header.time_stamp, prev_blk);
        blk.nonce += 1.into();
    }

    blk
}

pub fn verify_consensus(blk: &Block, prev_blk: &Block) -> Result<()> {
    ensure!(
        blk.diff == compute_diff(blk.header.time_stamp, prev_blk),
        "Invalid difficult."
    );
    ensure!(blk.to_digest() <= diff_to_h256(blk.diff), "Invalid nonce");

    Ok(())
}
