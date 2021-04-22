use crate::block::{BlockLoaderTrait, BlockTrait};
use kvdb::{DBKey, DBTransaction, KeyValueDB};
use serde::{Deserialize, Serialize};
use slimchain_chain::role::Role;
use slimchain_common::{
    basic::{AccountData, Address, BlockHeight, StateValue, H256},
    error::{bail, Context as _, Error, Result},
};
use slimchain_tx_state::{TrieNode, TxStateUpdate, TxStateView};
use slimchain_utils::{
    record_event,
    serde::{binary_decode, binary_encode},
};
use std::{path::Path, sync::Arc};

pub use slimchain_chain::db::{
    block_height_to_db_key, h256_to_db_key, str_to_db_key, u64_to_db_key, BLOCK_DB_COL, LOG_DB_COL,
    META_DB_COL, STATE_DB_COL, TOTAL_COLS,
};

pub struct DB {
    db: Box<dyn KeyValueDB>,
}

pub type DBPtr = Arc<DB>;

impl DB {
    pub fn open_or_create(path: &Path, enable_statistics: bool) -> Result<Arc<Self>> {
        info!("Open database at {}", path.display());
        let mut cfg = kvdb_rocksdb::DatabaseConfig::with_columns(TOTAL_COLS);
        cfg.enable_statistics = enable_statistics;
        let db = kvdb_rocksdb::Database::open(&cfg, &path.to_string_lossy())?;
        Ok(Arc::new(Self { db: Box::new(db) }))
    }

    pub fn open_or_create_in_dir(
        dir: &Path,
        role: Role,
        enable_statistics: bool,
    ) -> Result<Arc<Self>> {
        let db_file = match role {
            Role::Client => "client.db",
            Role::Miner => "miner.db",
            _ => bail!("Role can only be client or miner."),
        };
        Self::open_or_create(&dir.join(db_file), enable_statistics)
    }

    pub fn get_object<T: for<'de> Deserialize<'de>>(
        &self,
        col: u32,
        key: &DBKey,
    ) -> Result<Option<T>> {
        self.db
            .get(col, key)
            .map_err(Error::msg)?
            .map(|bin| binary_decode::<T>(&bin[..]))
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

impl Drop for DB {
    fn drop(&mut self) {
        let stats = self.db.io_stats(kvdb::IoStatsKind::Overall);
        record_event!("db-io-stats",
            "transactions": stats.transactions,
            "reads": stats.reads,
            "cache_reads": stats.cache_reads,
            "writes": stats.writes,
            "bytes_read": stats.bytes_read,
            "cache_read_bytes": stats.cache_read_bytes,
            "bytes_written": stats.bytes_written,
            "span": stats.span,
        );
    }
}

impl<Block: BlockTrait + for<'de> Deserialize<'de>> BlockLoaderTrait<Block> for DB {
    #[tracing::instrument(level = "debug", skip(self), err)]
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block> {
        self.get_existing_object(BLOCK_DB_COL, &block_height_to_db_key(height))
            .with_context(|| format!("Failed to get block from the database. height: {}", height))
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
        let bin = binary_encode(value)?;
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
