use crate::{
    access_map::AccessMap,
    block::BlockTrait,
    db::{DBPtr, Transaction},
    latest::set_latest_block_header,
    loader::BlockLoaderTrait,
};
use serde::Deserialize;
use slimchain_common::{
    basic::{BlockHeight, ShardId, H256},
    error::{Context as _, Result},
};
use slimchain_tx_state::{InShardData, OutShardData, StorageTxTrie, TxTrie, TxTrieTrait};

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

impl<Block: BlockTrait + for<'de> Deserialize<'de>> Snapshot<Block, TxTrie> {
    fn write_db_tx(&self) -> Result<Transaction> {
        let mut tx = Transaction::with_capacity(3);
        tx.insert_block_height(self.current_height())?;
        tx.insert_access_map(&self.access_map)?;
        tx.insert_tx_trie(&self.tx_trie)?;
        Ok(tx)
    }

    pub fn write_sync(&self, db: &DBPtr) -> Result<()> {
        db.write_sync(self.write_db_tx()?)
    }

    pub async fn write_async(&self, db: &DBPtr) -> Result<()> {
        db.write_async(self.write_db_tx()?).await
    }

    pub fn load_from_db(db: &DBPtr, state_len: usize) -> Result<Self> {
        if let Some(height) = db.get_block_height()? {
            let recent_blocks = load_recent_blocks::<Block>(db, height, state_len)?;
            let tx_trie = db.get_tx_trie()?;
            let access_map = db.get_access_map()?;
            assert_eq!(height, access_map.latest_block_height());
            Ok(Self::new(recent_blocks, tx_trie, access_map))
        } else {
            let genesis_block = Block::genesis_block();
            set_latest_block_header(&genesis_block);
            Ok(Self::genesis_snapshot(
                Default::default(),
                genesis_block,
                state_len,
            ))
        }
    }
}

impl<Block: BlockTrait + for<'de> Deserialize<'de>> Snapshot<Block, StorageTxTrie> {
    fn write_db_tx(&self) -> Result<Transaction> {
        let mut tx = Transaction::with_capacity(4);
        tx.insert_block_height(self.current_height())?;
        tx.insert_shard_id(self.tx_trie.get_shard_id())?;
        tx.insert_access_map(&self.access_map)?;
        tx.insert_out_shard_data(self.tx_trie.get_out_shard_data())?;
        Ok(tx)
    }

    pub fn write_sync(&self, db: &DBPtr) -> Result<()> {
        db.write_sync(self.write_db_tx()?)
    }

    pub async fn write_async(&self, db: &DBPtr) -> Result<()> {
        db.write_async(self.write_db_tx()?).await
    }

    pub fn load_from_db(db: &DBPtr, state_len: usize, shard_id: ShardId) -> Result<Self> {
        if let Some(height) = db.get_block_height()? {
            let recent_blocks = load_recent_blocks::<Block>(db, height, state_len)?;
            let root = recent_blocks
                .back()
                .context("Failed to access the latest block.")?
                .state_root();
            let out_shard_data = db.get_out_shard_data()?;
            let tx_trie =
                StorageTxTrie::new(shard_id, InShardData::new(db.clone(), root), out_shard_data);
            let access_map = db.get_access_map()?;
            assert_eq!(height, access_map.latest_block_height());
            Ok(Self::new(recent_blocks, tx_trie, access_map))
        } else {
            let tx_trie = StorageTxTrie::new(
                shard_id,
                InShardData::new(db.clone(), H256::zero()),
                OutShardData::default(),
            );
            let genesis_block = Block::genesis_block();
            set_latest_block_header(&genesis_block);
            Ok(Self::genesis_snapshot(tx_trie, genesis_block, state_len))
        }
    }
}

fn load_recent_blocks<Block: BlockTrait + for<'de> Deserialize<'de>>(
    db: &DBPtr,
    block_height: BlockHeight,
    state_len: usize,
) -> Result<im::Vector<Block>> {
    let mut height = block_height;
    let mut out = im::Vector::new();
    while height.0 > 0 && out.len() < state_len {
        let blk = db.get_block(height)?;
        out.push_front(blk);
        height.0 -= 1;
    }

    if out.len() < state_len {
        debug_assert!(height.is_zero());
        let blk = db.get_block(height)?;
        out.push_front(blk);
    }

    Ok(out)
}
