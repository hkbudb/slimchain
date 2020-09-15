use crate::{
    access_map::AccessMap,
    block::BlockTrait,
    block_proposal::BlockProposal,
    loader::{BlockLoaderTrait, TxLoaderTrait},
};
use kvdb::{DBKey, DBTransaction, DBValue, KeyValueDB};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{AccountData, Address, BlockHeight, ShardId, StateValue, H256},
    error::{Context as _, Error, Result},
    tx::TxTrait,
};
use slimchain_tx_state::{OutShardData, TrieNode, TxStateUpdate, TxStateView, TxTrie};
use std::{path::Path, sync::Arc};

const TOTAL_COLS: u32 = 5;
// store meta data
const META_DB_COL: u32 = 0;
// store block height <-> block
const BLOCK_DB_COL: u32 = 1;
// store tx_hash <-> tx
const TX_DB_COL: u32 = 2;
// store state addr <-> node
const STATE_DB_COL: u32 = 3;

static META_BLOCK_HEIGHT_KEY: Lazy<DBKey> = Lazy::new(|| str_to_db_key("height"));
static META_SHARD_ID_KEY: Lazy<DBKey> = Lazy::new(|| str_to_db_key("shard-id"));
static META_ACCESS_MAP_KEY: Lazy<DBKey> = Lazy::new(|| str_to_db_key("access-map"));
static META_TX_TRIE_KEY: Lazy<DBKey> = Lazy::new(|| str_to_db_key("tx-trie"));
static META_OUT_SHARD_DATA_KEY: Lazy<DBKey> = Lazy::new(|| str_to_db_key("out-shard-data"));

#[inline]
fn h256_to_db_key(input: H256) -> DBKey {
    debug_assert!(!input.is_zero());
    DBKey::from_buf(input.to_fixed_bytes())
}

#[inline]
fn block_height_to_db_key(height: BlockHeight) -> DBKey {
    let mut key = DBKey::new();
    key.extend_from_slice(&height.to_le_bytes()[..]);
    key
}

#[inline]
fn str_to_db_key(input: &str) -> DBKey {
    let mut key = DBKey::new();
    key.extend_from_slice(input.as_bytes());
    key
}

pub struct DB {
    db: Box<dyn KeyValueDB>,
}

pub type DBPtr = Arc<DB>;

impl DB {
    pub fn open_or_create(path: &Path) -> Result<Arc<Self>> {
        info!("Open database at {}", path.display());
        let cfg = kvdb_rocksdb::DatabaseConfig::with_columns(TOTAL_COLS);
        let db = kvdb_rocksdb::Database::open(&cfg, &path.to_string_lossy())?;
        Ok(Arc::new(Self { db: Box::new(db) }))
    }

    #[cfg(test)]
    pub fn load_test() -> Arc<Self> {
        let db = kvdb_memorydb::create(TOTAL_COLS);
        Arc::new(Self { db: Box::new(db) })
    }

    fn get_raw(&self, col: u32, key: &DBKey) -> Result<Option<DBValue>> {
        self.db.get(col, key).map_err(Error::msg)
    }

    pub fn get_object<T: for<'de> Deserialize<'de>>(
        &self,
        col: u32,
        key: &DBKey,
    ) -> Result<Option<T>> {
        self.get_raw(col, key)?
            .map(|bin| postcard::from_bytes::<T>(&bin[..]).map_err(Error::msg))
            .transpose()
    }

    pub fn get_existing_object<T: for<'de> Deserialize<'de>>(
        &self,
        col: u32,
        key: &DBKey,
    ) -> Result<T> {
        self.get_object(col, key)?
            .context("Object not available in the database.")
    }

    pub fn get_block_height(&self) -> Result<Option<BlockHeight>> {
        self.get_object(META_DB_COL, &(*META_BLOCK_HEIGHT_KEY))
    }

    pub fn get_shard_id(&self) -> Result<Option<ShardId>> {
        self.get_object(META_DB_COL, &(*META_SHARD_ID_KEY))
    }

    pub fn get_access_map(&self) -> Result<AccessMap> {
        self.get_existing_object(META_DB_COL, &(*META_ACCESS_MAP_KEY))
    }

    pub fn get_tx_trie(&self) -> Result<TxTrie> {
        self.get_existing_object(META_DB_COL, &(*META_TX_TRIE_KEY))
    }

    pub fn get_out_shard_data(&self) -> Result<OutShardData> {
        self.get_existing_object(META_DB_COL, &(*META_OUT_SHARD_DATA_KEY))
    }

    pub fn write_sync(&self, tx: Transaction) -> Result<()> {
        self.db.write(tx.inner).map_err(Error::msg)
    }

    pub async fn write_async(self: &Arc<Self>, tx: Transaction) -> Result<()> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.db.write(tx.inner))
            .await?
            .map_err(Error::msg)
    }
}

impl<Block: BlockTrait + for<'de> Deserialize<'de>> BlockLoaderTrait<Block> for DB {
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block> {
        self.get_existing_object(BLOCK_DB_COL, &block_height_to_db_key(height))
    }
}

impl<Tx: TxTrait + for<'de> Deserialize<'de>> TxLoaderTrait<Tx> for DB {
    fn get_tx(&self, tx_hash: H256) -> Result<Tx> {
        self.get_existing_object(TX_DB_COL, &h256_to_db_key(tx_hash))
    }
}

impl TxStateView for DB {
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>> {
        self.get_existing_object(STATE_DB_COL, &h256_to_db_key(node_address))
    }

    fn state_trie_node(
        &self,
        _acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>> {
        self.get_existing_object(STATE_DB_COL, &h256_to_db_key(node_address))
    }
}

#[derive(Default)]
pub struct Transaction {
    inner: DBTransaction,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: DBTransaction::with_capacity(cap),
        }
    }

    pub fn insert_object<T: Serialize>(&mut self, col: u32, key: &DBKey, value: &T) -> Result<()> {
        let bin = postcard::to_allocvec(value)?;
        self.inner.put_vec(col, key, bin);
        Ok(())
    }

    pub fn insert_block<Block: BlockTrait + Serialize>(&mut self, block: &Block) -> Result<()> {
        self.insert_object(
            BLOCK_DB_COL,
            &block_height_to_db_key(block.block_height()),
            block,
        )
    }

    pub fn insert_tx<Tx: TxTrait + Serialize>(&mut self, tx_hash: H256, tx: &Tx) -> Result<()> {
        self.insert_object(TX_DB_COL, &h256_to_db_key(tx_hash), tx)
    }

    pub fn insert_block_proposal<Block: BlockTrait + Serialize, Tx: TxTrait + Serialize>(
        &mut self,
        block_proposal: BlockProposal<Block, Tx>,
    ) -> Result<()> {
        let (blk, txs) = block_proposal.unpack();
        self.insert_block(&blk)?;
        for (&tx_hash, tx) in blk.tx_list().iter().zip(txs.iter()) {
            debug_assert_eq!(tx_hash, tx.to_digest());
            self.insert_tx(tx_hash, tx)?;
        }
        Ok(())
    }

    pub fn update_state(&mut self, update: &TxStateUpdate) -> Result<()> {
        for (&addr, node) in update.acc_nodes.iter() {
            self.insert_object(STATE_DB_COL, &h256_to_db_key(addr), node)?;
        }

        for (_, state_update) in update.state_nodes.iter() {
            for (&addr, node) in state_update.iter() {
                self.insert_object(STATE_DB_COL, &h256_to_db_key(addr), node)?;
            }
        }

        Ok(())
    }

    pub fn insert_block_height(&mut self, block_height: BlockHeight) -> Result<()> {
        self.insert_object(META_DB_COL, &(*META_BLOCK_HEIGHT_KEY), &block_height)
    }

    pub fn insert_shard_id(&mut self, shard_id: ShardId) -> Result<()> {
        self.insert_object(META_DB_COL, &(*META_SHARD_ID_KEY), &shard_id)
    }

    pub fn insert_access_map(&mut self, access_map: &AccessMap) -> Result<()> {
        self.insert_object(META_DB_COL, &(*META_ACCESS_MAP_KEY), access_map)
    }

    pub fn insert_tx_trie(&mut self, tx_trie: &TxTrie) -> Result<()> {
        self.insert_object(META_DB_COL, &(*META_TX_TRIE_KEY), tx_trie)
    }

    pub fn insert_out_shard_data(&mut self, data: &OutShardData) -> Result<()> {
        self.insert_object(META_DB_COL, &(*META_OUT_SHARD_DATA_KEY), data)
    }
}
