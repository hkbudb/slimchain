use super::TxTrieDiff;
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
};
use slimchain_merkle_trie::prelude::*;

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::From,
    derive_more::Into,
)]
pub struct OutShardData(pub im::HashMap<Address, PartialTrie>);

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
pub struct TxTrieWithSharding {
    pub shard_id: ShardId,
    pub in_shard: InShardData,
    pub out_shard: OutShardData,
}

impl TxTrieWithSharding {
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

    pub fn get_out_shard_data(&self) -> &OutShardData {
        &self.out_shard
    }

    pub fn apply_diff(&mut self, diff: &TxTrieDiff, check_hash: bool) -> Result<()> {
        for (&acc_addr, acc_trie_diff) in diff.acc_trie_diffs.iter() {
            if self.shard_id.contains(acc_addr) {
                continue;
            }

            match self.out_shard.entry(acc_addr) {
                im::hashmap::Entry::Occupied(mut o) => {
                    let acc_state_trie =
                        apply_diff(o.get(), &acc_trie_diff.state_trie_diff, check_hash)?;
                    *o.get_mut() = acc_state_trie;
                }
                im::hashmap::Entry::Vacant(v) => {
                    let acc_state_trie = acc_trie_diff.state_trie_diff.to_standalone_trie()?;
                    if check_hash {
                        ensure!(
                            self.in_shard.get_acc_state_root(acc_addr)?
                                == acc_state_trie.root_hash(),
                            "TxTrieWithSharding#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    } else {
                        debug_assert_eq!(
                            self.in_shard.get_acc_state_root(acc_addr)?,
                            acc_state_trie.root_hash(),
                            "TxTrieWithSharding#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    }
                    v.insert(acc_state_trie);
                }
            }
        }

        Ok(())
    }

    pub fn apply_writes(&mut self, writes: &TxWriteData) -> Result<TxStateUpdate> {
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
                    let acc_state_trie = self.out_shard.entry(acc_addr).or_default();

                    if acc_data.reset_values {
                        *acc_state_trie = PartialTrie::new();
                    }

                    let mut state_write_ctx = WritePartialTrieContext::new(acc_state_trie.clone());
                    for (k, v) in acc_data.values.iter() {
                        state_write_ctx.insert_with_value(k, v)?;
                    }
                    *acc_state_trie = state_write_ctx.finish();

                    acc_state_trie.root_hash()
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

    fn prune_helper(
        &mut self,
        acc_addr: Address,
        callback: impl FnOnce(&mut PartialTrie) -> Result<()>,
    ) -> Result<()> {
        if self.shard_id.contains(acc_addr) {
            return Ok(());
        }

        let mut entry = match self.out_shard.entry(acc_addr) {
            im::hashmap::Entry::Occupied(o) => o,
            im::hashmap::Entry::Vacant(_) => {
                bail!("TxTrieWithSharding#prune_helper: Account is already pruned");
            }
        };

        callback(entry.get_mut())?;

        if entry.get().can_be_pruned() {
            entry.remove();
        }

        Ok(())
    }

    pub fn prune_acc_state_key(&mut self, acc_addr: Address, key: StateKey) -> Result<()> {
        self.prune_helper(acc_addr, |trie| {
            *trie = prune_unused_key(trie, &key)?;
            Ok(())
        })
    }

    pub fn prune_acc_state_keys(
        &mut self,
        acc_addr: Address,
        keys: impl Iterator<Item = StateKey>,
    ) -> Result<()> {
        self.prune_helper(acc_addr, move |trie| {
            *trie = prune_unused_keys(trie, keys)?;
            Ok(())
        })
    }
}
