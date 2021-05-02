use crate::loader::TxLoaderTrait;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{BlockHeight, H256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{ensure, Result},
    tx::TxTrait,
    utils::derive_more::{Deref, DerefMut},
};
use std::iter::FromIterator;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: BlockHeight,
    pub prev_blk_hash: H256,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub time_stamp: DateTime<Utc>,
    pub tx_list: BlockTxList,
    pub state_root: H256,
}

pub fn block_header_to_digest(
    height: BlockHeight,
    prev_blk_hash: H256,
    time_stamp: DateTime<Utc>,
    tx_list_root: H256,
    state_root: H256,
) -> H256 {
    let mut hash_state = default_blake2().to_state();
    hash_state.update(height.to_digest().as_bytes());
    hash_state.update(prev_blk_hash.as_bytes());
    hash_state.update(time_stamp.timestamp_millis().to_digest().as_bytes());
    hash_state.update(tx_list_root.as_bytes());
    hash_state.update(state_root.as_bytes());
    let hash = hash_state.finalize();
    blake2b_hash_to_h256(hash)
}

impl Digestible for BlockHeader {
    fn to_digest(&self) -> H256 {
        block_header_to_digest(
            self.height,
            self.prev_blk_hash,
            self.time_stamp,
            self.tx_list.to_digest(),
            self.state_root,
        )
    }
}

impl BlockHeader {
    pub fn new(
        height: BlockHeight,
        prev_blk_hash: H256,
        time_stamp: DateTime<Utc>,
        tx_list: BlockTxList,
        state_root: H256,
    ) -> Self {
        let time_stamp = Utc.timestamp_millis(time_stamp.timestamp_millis());
        Self {
            height,
            prev_blk_hash,
            time_stamp,
            tx_list,
            state_root,
        }
    }

    pub fn set_ts(&mut self, time_stamp: DateTime<Utc>) {
        self.time_stamp = Utc.timestamp_millis(time_stamp.timestamp_millis());
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct BlockTxList(pub Vec<H256>);

impl Digestible for BlockTxList {
    fn to_digest(&self) -> H256 {
        if self.is_empty() {
            return H256::zero();
        }

        let mut hash_state = default_blake2().to_state();
        for tx in self.iter() {
            hash_state.update(tx.as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl FromIterator<H256> for BlockTxList {
    fn from_iter<T: IntoIterator<Item = H256>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a, Tx: TxTrait> FromIterator<&'a Tx> for BlockTxList {
    fn from_iter<T: IntoIterator<Item = &'a Tx>>(iter: T) -> Self {
        iter.into_iter().map(|tx| tx.to_digest()).collect()
    }
}

impl BlockTxList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self(Vec::with_capacity(cap))
    }

    pub fn to_txs<Tx: TxTrait>(&self, loader: &impl TxLoaderTrait<Tx>) -> Result<Vec<Tx>> {
        self.iter().map(|tx_hash| loader.get_tx(*tx_hash)).collect()
    }
}

pub trait BlockTrait: Digestible + Clone + Sized + Send + Sync {
    fn genesis_block() -> Self;
    fn block_header(&self) -> &BlockHeader;
    fn block_header_mut(&mut self) -> &mut BlockHeader;

    fn block_height(&self) -> BlockHeight {
        self.block_header().height
    }
    fn prev_blk_hash(&self) -> H256 {
        self.block_header().prev_blk_hash
    }
    fn time_stamp(&self) -> DateTime<Utc> {
        self.block_header().time_stamp
    }
    fn tx_root(&self) -> H256 {
        self.block_header().tx_list.to_digest()
    }
    fn tx_list(&self) -> &BlockTxList {
        &self.block_header().tx_list
    }
    fn tx_list_mut(&mut self) -> &mut BlockTxList {
        &mut self.block_header_mut().tx_list
    }
    fn state_root(&self) -> H256 {
        self.block_header().state_root
    }

    fn verify_block_header(&self, prev_blk: &Self) -> Result<()> {
        ensure!(
            self.block_height() == prev_blk.block_height().next_height(),
            "Invalid block height."
        );
        ensure!(
            self.prev_blk_hash() == prev_blk.to_digest(),
            "Invalid previous block hash."
        );
        ensure!(
            self.time_stamp() >= prev_blk.time_stamp(),
            "Invalid timestamp."
        );
        ensure!(
            self.time_stamp() <= Utc::now() + chrono::Duration::seconds(30),
            "Future timestamp is not allowed."
        );
        Ok(())
    }
}
