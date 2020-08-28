use crate::block::{BlockHeader, BlockTrait, BlockTxList};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::H256,
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::Result,
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    header: BlockHeader,
}

impl Digestible for Block {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.header.to_digest().as_bytes());
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
        }
    }

    fn block_header(&self) -> &BlockHeader {
        &self.header
    }

    fn block_header_mut(&mut self) -> &mut BlockHeader {
        &mut self.header
    }
}

pub fn create_new_block(header: BlockHeader, _prev_blk: &Block) -> Block {
    Block { header }
}

pub fn verify_consensus(_blk: &Block, _prev_blk: &Block) -> Result<()> {
    // the consensus is verified by external Raft network protocol.
    Ok(())
}
