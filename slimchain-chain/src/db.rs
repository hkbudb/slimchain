use crate::{
    block::BlockTrait,
    loader::{BlockLoaderTrait, TxLoaderTrait},
    role::Role,
};
use kvdb::{DBKey, DBTransaction, KeyValueDB};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{AccountData, Address, BlockHeight, StateValue, H256},
    error::{Context as _, Error, Result},
    tx::TxTrait,
};
use slimchain_tx_state::{TrieNode, TxStateUpdate, TxStateView};
use std::{path::Path, sync::Arc};

pub const TOTAL_COLS: u32 = 5;
// store meta data
pub const META_DB_COL: u32 = 0;
// store block height <-> block
pub const BLOCK_DB_COL: u32 = 1;
// store tx_hash <-> tx
pub const TX_DB_COL: u32 = 2;
// store state addr <-> node
pub const STATE_DB_COL: u32 = 3;
// store log_idx <-> log
pub const LOG_DB_COL: u32 = 4;

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

#[inline]
fn u64_to_db_key(input: u64) -> DBKey {
    let mut key = DBKey::new();
    key.extend_from_slice(&input.to_le_bytes()[..]);
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

    pub fn open_or_create_in_dir(dir: &Path, role: Role) -> Result<Arc<Self>> {
        let db_file = match role {
            Role::Client => "client.db",
            Role::Miner => "miner.db",
            Role::Storage(_) => "storage.db",
        };
        Self::open_or_create(&dir.join(db_file))
    }

    #[cfg(test)]
    pub fn load_test() -> Arc<Self> {
        let db = kvdb_memorydb::create(TOTAL_COLS);
        Arc::new(Self { db: Box::new(db) })
    }

    pub fn get_object<T: for<'de> Deserialize<'de>>(
        &self,
        col: u32,
        key: &DBKey,
    ) -> Result<Option<T>> {
        self.db
            .get(col, key)
            .map_err(Error::msg)?
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

    pub fn get_meta_object<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>> {
        self.get_object(META_DB_COL, &str_to_db_key(key))
    }

    pub fn get_existing_meta_object<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T> {
        self.get_existing_object(META_DB_COL, &str_to_db_key(key))
    }

    pub fn get_log_object<T: for<'de> Deserialize<'de>>(&self, idx: u64) -> Result<Option<T>> {
        self.get_object(LOG_DB_COL, &u64_to_db_key(idx))
    }

    pub fn get_table_size(&self, col: u32) -> usize {
        self.db.iter(col).map(|(k, v)| k.len() + v.len()).sum()
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
    #[tracing::instrument(level = "debug", skip(self), err)]
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block> {
        self.get_existing_object(BLOCK_DB_COL, &block_height_to_db_key(height))
            .with_context(|| format!("Failed to get block from the database. height: {}", height))
    }
}

impl<Tx: TxTrait + for<'de> Deserialize<'de>> TxLoaderTrait<Tx> for DB {
    #[tracing::instrument(level = "debug", skip(self), err)]
    fn get_tx(&self, tx_hash: H256) -> Result<Tx> {
        self.get_existing_object(TX_DB_COL, &h256_to_db_key(tx_hash))
            .with_context(|| format!("Failed to get tx from the database. tx_hash: {}", tx_hash))
    }
}

impl TxStateView for DB {
    #[tracing::instrument(level = "debug", skip(self), err)]
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>> {
        self.get_existing_object(STATE_DB_COL, &h256_to_db_key(node_address))
            .with_context(|| {
                format!(
                    "Failed to get account trie node from the database. node: {}",
                    node_address
                )
            })
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn state_trie_node(
        &self,
        acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>> {
        self.get_existing_object(STATE_DB_COL, &h256_to_db_key(node_address))
            .with_context(|| {
                format!(
                    "Failed to get state trie node from the database. acc: {}, node: {}",
                    acc_address, node_address
                )
            })
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

    pub fn insert_meta_object<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        self.insert_object(META_DB_COL, &str_to_db_key(key), value)
    }

    pub fn insert_log_object<T: Serialize>(&mut self, idx: u64, value: &T) -> Result<()> {
        self.insert_object(LOG_DB_COL, &u64_to_db_key(idx), value)
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

    pub fn delete_object(&mut self, col: u32, key: &DBKey) {
        self.inner.delete(col, key);
    }

    pub fn delete_log_object(&mut self, idx: u64) {
        self.delete_object(LOG_DB_COL, &u64_to_db_key(idx))
    }
}
