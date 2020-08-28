use crate::{access_map::AccessMap, block::BlockTrait};
use slimchain_common::{basic::BlockHeight, error::Result};
use slimchain_tx_state::TxTrieTrait;

#[derive(Clone)]
pub struct Snapshot<Block: BlockTrait, TxTrie: TxTrieTrait> {
    pub(crate) recent_blocks: im::Vector<Block>,
    pub(crate) tx_trie: TxTrie,
    pub(crate) access_map: AccessMap,
}

impl<Block: BlockTrait, TxTrie: TxTrieTrait> Snapshot<Block, TxTrie> {
    pub fn new(recent_blocks: im::Vector<Block>, tx_trie: TxTrie, access_map: AccessMap) -> Self {
        Self {
            recent_blocks,
            tx_trie,
            access_map,
        }
    }

    pub fn genesis_snapshot(tx_trie: TxTrie, genesis_block: Block, state_len: usize) -> Self {
        Self {
            recent_blocks: im::vector![genesis_block],
            tx_trie,
            access_map: AccessMap::new(state_len),
        }
    }

    pub fn current_height(&self) -> BlockHeight {
        self.access_map.latest_block_height()
    }

    pub fn get_latest_block(&self) -> Option<&Block> {
        self.recent_blocks.back()
    }

    pub fn get_block(&self, height: BlockHeight) -> Option<&Block> {
        let idx = height - self.access_map.oldest_block_height();
        if idx.is_negative() {
            None
        } else {
            self.recent_blocks.get(idx.0 as usize)
        }
    }

    pub fn commit_block(&mut self, block: Block) {
        self.recent_blocks.push_back(block);
    }

    pub fn remove_oldest_block(&mut self) -> Result<()> {
        let oldest_height = self.access_map.oldest_block_height();
        self.access_map
            .remove_oldest_block()
            .prune_tx_trie(&mut self.tx_trie)?;
        if oldest_height != self.access_map.oldest_block_height() {
            self.recent_blocks.pop_front();
        }
        Ok(())
    }
}
