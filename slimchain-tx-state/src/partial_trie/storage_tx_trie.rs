use super::{TxTrieDiff, TxTrieTrait, TxWriteSetTrie};
use crate::{
    view::{
        trie_view_sync::{AccountTrieView, StateTrieView},
        TxStateView,
    },
    write::TxStateUpdate,
};
use alloc::{format, sync::Arc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{AccountData, Address, ShardId, StateKey, H256},
    error::{bail, ensure, Result},
    rw_set::TxWriteData,
    utils::derive_more::{Deref, DerefMut},
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageAccountTrie {
    state_trie: PartialTrie,
    reset_values_flag: bool,
}

impl StorageAccountTrie {
    pub fn new(state_trie: PartialTrie, reset_values_flag: bool) -> Self {
        Self {
            state_trie,
            reset_values_flag,
        }
    }

    pub fn create_from_state_trie(state_trie: PartialTrie) -> Self {
        Self {
            state_trie,
            reset_values_flag: false,
        }
    }

    pub fn get_state_trie(&self) -> &PartialTrie {
        &self.state_trie
    }

    pub fn set_state_trie(&mut self, state_trie: PartialTrie) {
        self.state_trie = state_trie;
    }

    pub fn get_reset_values(&self) -> bool {
        self.reset_values_flag
    }

    pub fn set_reset_values(&mut self, flag: bool) {
        self.reset_values_flag = flag;
    }

    pub fn can_be_pruned(&self) -> bool {
        (!self.reset_values_flag) && self.state_trie.can_be_pruned()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct OutShardData(pub im::HashMap<Address, StorageAccountTrie>);

#[derive(Clone)]
pub struct InShardData {
    pub root: H256,
    pub state_view: Arc<dyn TxStateView + Sync + Send>,
}

impl InShardData {
    pub fn new(state_view: Arc<dyn TxStateView + Sync + Send>, root: H256) -> Self {
        Self { root, state_view }
    }

    pub fn get_account(&self, acc_addr: Address) -> Result<AccountData> {
        let (acc_data, _) = read_trie(
            &AccountTrieView::new(Arc::clone(&self.state_view)),
            self.root,
            &acc_addr,
        )?;
        Ok(acc_data.unwrap_or_default())
    }

    pub fn get_acc_state_root(&self, acc_addr: Address) -> Result<H256> {
        self.get_account(acc_addr).map(|acc| acc.acc_state_root)
    }

    pub fn get_acc_trie_view(&self) -> AccountTrieView {
        AccountTrieView::new(Arc::clone(&self.state_view))
    }

    pub fn get_state_trie_view(&self, acc_addr: Address) -> StateTrieView {
        StateTrieView::new(Arc::clone(&self.state_view), acc_addr)
    }
}

#[derive(Clone)]
pub struct StorageTxTrie {
    pub shard_id: ShardId,
    pub in_shard: InShardData,
    pub out_shard: OutShardData,
}

impl StorageTxTrie {
    pub fn new(shard_id: ShardId, in_shard: InShardData, out_shard: OutShardData) -> Self {
        Self {
            shard_id,
            in_shard,
            out_shard,
        }
    }

    pub fn get_shard_id(&self) -> ShardId {
        self.shard_id
    }

    pub fn get_state_root(&self) -> H256 {
        self.in_shard.root
    }

    pub fn get_out_shard_data(&self) -> &OutShardData {
        &self.out_shard
    }

    fn prune_helper(
        &mut self,
        acc_addr: Address,
        callback: impl FnOnce(&mut StorageAccountTrie) -> Result<()>,
    ) -> Result<()> {
        if self.shard_id.contains(acc_addr) {
            return Ok(());
        }

        let mut entry = match self.out_shard.entry(acc_addr) {
            im::hashmap::Entry::Occupied(o) => o,
            im::hashmap::Entry::Vacant(_) => {
                bail!("StorageTxTrie#prune_helper: Account is already pruned");
            }
        };

        callback(entry.get_mut())?;

        if entry.get().can_be_pruned() {
            entry.remove();
        }

        Ok(())
    }
}

impl TxTrieTrait for StorageTxTrie {
    fn root_hash(&self) -> H256 {
        self.in_shard.root
    }

    fn update_missing_branches(&mut self, fork: &TxWriteSetTrie) -> Result<()> {
        for (&acc_addr, fork_acc_trie) in fork.acc_tries.iter() {
            if self.shard_id.contains(acc_addr) {
                continue;
            }

            match self.out_shard.entry(acc_addr) {
                im::hashmap::Entry::Occupied(mut o) => {
                    let acc_state_trie = update_missing_branches(
                        o.get().get_state_trie(),
                        &fork_acc_trie.state_trie,
                        true,
                    )?;
                    o.get_mut().set_state_trie(acc_state_trie);
                }
                im::hashmap::Entry::Vacant(v) => {
                    let acc_state_trie = fork_acc_trie.state_trie.clone();
                    debug_assert_eq!(
                        self.in_shard.get_acc_state_root(acc_addr)?,
                        acc_state_trie.root_hash(),
                        "StorageTxTrie#update_missing_branches: Hash mismatched (address: {}).",
                        acc_addr
                    );
                    v.insert(StorageAccountTrie::create_from_state_trie(acc_state_trie));
                }
            }
        }

        Ok(())
    }

    fn apply_diff(&mut self, diff: &TxTrieDiff, check_hash: bool) -> Result<()> {
        for (&acc_addr, acc_trie_diff) in diff.acc_trie_diffs.iter() {
            if self.shard_id.contains(acc_addr) {
                continue;
            }

            match self.out_shard.entry(acc_addr) {
                im::hashmap::Entry::Occupied(mut o) => {
                    let acc_state_trie = apply_diff(
                        o.get().get_state_trie(),
                        &acc_trie_diff.state_trie_diff,
                        check_hash,
                    )?;
                    o.get_mut().set_state_trie(acc_state_trie);
                }
                im::hashmap::Entry::Vacant(v) => {
                    let acc_state_trie = acc_trie_diff.state_trie_diff.to_standalone_trie()?;
                    if check_hash {
                        ensure!(
                            self.in_shard.get_acc_state_root(acc_addr)?
                                == acc_state_trie.root_hash(),
                            "StorageTxTrie#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    } else {
                        debug_assert_eq!(
                            self.in_shard.get_acc_state_root(acc_addr)?,
                            acc_state_trie.root_hash(),
                            "StorageTxTrie#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    }
                    v.insert(StorageAccountTrie::create_from_state_trie(acc_state_trie));
                }
            }
        }

        Ok(())
    }

    fn apply_writes(&mut self, writes: &TxWriteData) -> Result<TxStateUpdate> {
        let mut updates = TxStateUpdate::default();
        let mut acc_write_ctx = WriteTrieContext::<Address, _, _>::new(
            self.in_shard.get_acc_trie_view(),
            self.in_shard.root,
        );

        for (&acc_addr, acc_data) in writes.iter() {
            let old_acc_data = self.in_shard.get_account(acc_addr)?;

            if self.shard_id.contains(acc_addr) {
                let acc_state_root = if acc_data.reset_values {
                    H256::zero()
                } else {
                    old_acc_data.acc_state_root
                };

                let mut state_write_ctx = WriteTrieContext::<StateKey, _, _>::new(
                    self.in_shard.get_state_trie_view(acc_addr),
                    acc_state_root,
                );
                for (k, v) in acc_data.values.iter() {
                    state_write_ctx.insert(k, *v)?;
                }

                let state_apply = state_write_ctx.changes();
                let acc_data = AccountData {
                    nonce: acc_data.nonce.unwrap_or(old_acc_data.nonce),
                    code: acc_data.code.clone().unwrap_or_else(|| old_acc_data.code),
                    acc_state_root: state_apply.root,
                };

                if !state_apply.nodes.is_empty() {
                    updates.state_nodes.insert(acc_addr, state_apply.nodes);
                }

                acc_write_ctx.insert(&acc_addr, acc_data)?;
            } else {
                let acc_state_root = if acc_data.values.is_empty() && !acc_data.reset_values {
                    // do not create out-shard trie if we do not update its values
                    old_acc_data.acc_state_root
                } else {
                    let acc_state = self.out_shard.entry(acc_addr).or_default();

                    debug_assert_eq!(
                        old_acc_data.acc_state_root,
                        acc_state.get_state_trie().root_hash(),
                        "StorageTxTrie#apply_writes: Hash mismatched between main trie and account trie (address: {}).",
                        acc_addr
                    );

                    if acc_data.reset_values {
                        acc_state.set_state_trie(PartialTrie::new());
                        acc_state.set_reset_values(true);
                    }

                    let mut state_write_ctx =
                        WritePartialTrieContext::new(acc_state.get_state_trie().clone());
                    for (k, v) in acc_data.values.iter() {
                        state_write_ctx.insert_with_value(k, v)?;
                    }
                    acc_state.set_state_trie(state_write_ctx.finish());

                    acc_state.get_state_trie().root_hash()
                };

                let acc_data = AccountData {
                    nonce: acc_data.nonce.unwrap_or(old_acc_data.nonce),
                    code: acc_data.code.clone().unwrap_or_else(|| old_acc_data.code),
                    acc_state_root,
                };
                acc_write_ctx.insert(&acc_addr, acc_data)?;
            }
        }

        let acc_apply = acc_write_ctx.changes();
        updates.root = acc_apply.root;
        updates.acc_nodes.extend(acc_apply.nodes.into_iter());

        self.in_shard.root = updates.root;
        Ok(updates)
    }

    fn prune_acc_nonce(&mut self, _acc_addr: Address) -> Result<()> {
        Ok(())
    }

    fn prune_acc_code(&mut self, _acc_addr: Address) -> Result<()> {
        Ok(())
    }

    fn prune_acc_state_key(&mut self, acc_addr: Address, key: StateKey) -> Result<()> {
        self.prune_helper(acc_addr, |acc_state| {
            let state_trie = prune_unused_key(acc_state.get_state_trie(), &key)?;
            acc_state.set_state_trie(state_trie);
            Ok(())
        })
    }

    fn prune_acc_state_keys(
        &mut self,
        acc_addr: Address,
        keys: impl Iterator<Item = StateKey>,
    ) -> Result<()> {
        self.prune_helper(acc_addr, move |acc_state| {
            let state_trie = prune_unused_keys(acc_state.get_state_trie(), keys)?;
            acc_state.set_state_trie(state_trie);
            Ok(())
        })
    }

    fn prune_acc_reset_values(&mut self, acc_addr: Address) -> Result<()> {
        self.prune_helper(acc_addr, move |acc_state| {
            acc_state.set_reset_values(false);
            Ok(())
        })
    }
}
