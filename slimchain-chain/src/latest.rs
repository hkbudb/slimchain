use crate::block::{BlockHeader, BlockTrait};
use arc_swap::ArcSwap;
use slimchain_common::basic::{BlockHeight, H256};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct LatestBlockHeader {
    header: ArcSwap<BlockHeader>,
}

pub type LatestBlockHeaderPtr = Arc<LatestBlockHeader>;

impl LatestBlockHeader {
    pub fn new(header: BlockHeader) -> Arc<Self> {
        Arc::new(Self {
            header: ArcSwap::from_pointee(header),
        })
    }

    pub fn new_from_block(block: &impl BlockTrait) -> Arc<Self> {
        Self::new(block.block_header().clone())
    }

    pub fn set(self: &Arc<Self>, header: BlockHeader) {
        self.header.store(Arc::new(header));
    }

    pub fn set_from_block(self: &Arc<Self>, block: &impl BlockTrait) {
        self.set(block.block_header().clone());
    }

    fn get_inner<T>(self: &Arc<Self>, f: impl FnOnce(&BlockHeader) -> T) -> T {
        let guard = self.header.load();
        f(guard.as_ref())
    }

    pub fn get(self: &Arc<Self>) -> Arc<BlockHeader> {
        self.header.load_full()
    }

    pub fn get_height(self: &Arc<Self>) -> BlockHeight {
        self.get_inner(|h| h.height)
    }

    pub fn get_height_and_state_root(self: &Arc<Self>) -> (BlockHeight, H256) {
        self.get_inner(|h| (h.height, h.state_root))
    }
}

#[derive(Debug, Default)]
pub struct LatestTxCount(AtomicUsize);

pub type LatestTxCountPtr = Arc<LatestTxCount>;

impl LatestTxCount {
    pub fn new(count: usize) -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(count)))
    }

    pub fn get(self: &Arc<Self>) -> usize {
        self.as_ref().0.load(Ordering::Acquire)
    }

    pub fn set(self: &Arc<Self>, count: usize) {
        self.as_ref().0.store(count, Ordering::Release)
    }

    pub fn add(self: &Arc<Self>, count: usize) {
        self.as_ref().0.fetch_add(count, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latest_block() {
        let mut block = crate::consensus::raft::Block::genesis_block();
        let latest_blk_header = LatestBlockHeader::new_from_block(&block);

        block.block_header_mut().height = 2.into();
        latest_blk_header.set_from_block(&block);
        assert_eq!(block.block_header(), latest_blk_header.get().as_ref());
        assert_eq!(BlockHeight::from(2), latest_blk_header.get_height());
        assert_eq!(
            (BlockHeight::from(2), H256::zero()),
            latest_blk_header.get_height_and_state_root(),
        );

        block.block_header_mut().height = 3.into();
        latest_blk_header.set_from_block(&block);
        assert_eq!(block.block_header(), latest_blk_header.get().as_ref());
        assert_eq!(BlockHeight::from(3), latest_blk_header.get_height());
        assert_eq!(
            (BlockHeight::from(3), H256::zero()),
            latest_blk_header.get_height_and_state_root(),
        );
    }

    #[test]
    fn test_latest_tx_count() {
        let cnt = LatestTxCount::new(1);
        assert_eq!(cnt.get(), 1);
        cnt.set(2);
        assert_eq!(cnt.get(), 2);
        cnt.add(3);
        assert_eq!(cnt.get(), 5);
    }
}
