use serde::{Deserialize, Serialize};
use slimchain_chain::{
    access_map::AccessMap,
    block::BlockTrait,
    db::{DBPtr, Transaction},
    latest::{LatestBlockHeader, LatestBlockHeaderPtr},
    snapshot::load_recent_blocks,
};
use slimchain_common::{
    basic::BlockHeight,
    error::{Context as _, Result},
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot<Block: BlockTrait> {
    pub(crate) recent_blocks: imbl::Vector<Block>,
    pub(crate) access_map: AccessMap,
}

impl<Block: BlockTrait> Snapshot<Block> {
    pub fn new(recent_blocks: imbl::Vector<Block>, access_map: AccessMap) -> Self {
        Self {
            recent_blocks,
            access_map,
        }
    }

    pub fn genesis_snapshot(genesis_block: Block, state_len: usize) -> Self {
        Self {
            recent_blocks: imbl::vector![genesis_block],
            access_map: AccessMap::new(state_len),
        }
    }

    pub fn current_height(&self) -> BlockHeight {
        self.access_map.latest_block_height()
    }

    pub fn get_latest_block(&self) -> Option<&Block> {
        self.recent_blocks.back()
    }

    pub fn to_latest_block_header(&self) -> LatestBlockHeaderPtr {
        LatestBlockHeader::new_from_block(
            self.get_latest_block()
                .expect("Failed to get the latest block."),
        )
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
        let _ = self.access_map.remove_oldest_block();
        if oldest_height != self.access_map.oldest_block_height() {
            self.recent_blocks.pop_front();
        }
        Ok(())
    }
}

impl<Block: BlockTrait + for<'de> Deserialize<'de>> Snapshot<Block> {
    pub fn write_db_tx(&self) -> Result<Transaction> {
        debug!("Saving snapshot...");
        let mut tx = Transaction::with_capacity(3);
        tx.insert_meta_object("height", &self.current_height())?;
        tx.insert_meta_object("access-map", &self.access_map)?;
        Ok(tx)
    }

    pub fn write_sync(&self, db: &DBPtr) -> Result<()> {
        db.write_sync(self.write_db_tx()?)
    }

    pub async fn write_async(&self, db: &DBPtr) -> Result<()> {
        db.write_async(self.write_db_tx()?).await
    }

    pub fn load_from_db(db: &DBPtr, state_len: usize) -> Result<Self> {
        debug!("Loading snapshot...");
        if let Some(height) = db
            .get_meta_object("height")
            .context("Failed to get block height from the database.")?
        {
            let recent_blocks = load_recent_blocks::<Block>(db, height, state_len)?;
            let access_map: AccessMap = db
                .get_existing_meta_object("access-map")
                .context("Failed to get access map from the database.")?;
            assert_eq!(height, access_map.latest_block_height());
            Ok(Self::new(recent_blocks, access_map))
        } else {
            let genesis_block = Block::genesis_block();
            Ok(Self::genesis_snapshot(genesis_block, state_len))
        }
    }
}
