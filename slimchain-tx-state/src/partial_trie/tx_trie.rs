use super::{AccountTrieDiff, AccountWriteSetTrie, TxTrieDiff, TxTrieTrait, TxWriteSetTrie};
use crate::write::TxStateUpdate;
use alloc::format;
use crossbeam_utils::atomic::AtomicCell;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, StateKey, H256},
    collections::HashMap,
    digest::Digestible,
    error::{bail, ensure, Result},
    rw_set::{AccountWriteData, TxWriteData},
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct AccountTrie {
    nonce: Nonce,
    code_hash: H256,
    state_trie: PartialTrie,
    #[serde(skip)]
    acc_hash: AtomicCell<Option<H256>>,
}

impl Clone for AccountTrie {
    fn clone(&self) -> Self {
        Self {
            nonce: self.nonce,
            code_hash: self.code_hash,
            state_trie: self.state_trie.clone(),
            acc_hash: AtomicCell::new(self.acc_hash.load()),
        }
    }
}

impl PartialEq for AccountTrie {
    fn eq(&self, other: &Self) -> bool {
        self.nonce == other.nonce
            && self.code_hash == other.code_hash
            && self.state_trie == other.state_trie
    }
}

impl Eq for AccountTrie {}

impl AccountTrie {
    fn reset_acc_hash(&mut self) {
        self.acc_hash.store(None);
    }

    pub fn new(nonce: Nonce, code_hash: H256, state_trie: PartialTrie) -> Self {
        Self {
            nonce,
            code_hash,
            state_trie,
            acc_hash: AtomicCell::new(None),
        }
    }

    fn acc_hash_inner(&self) -> H256 {
        let state_hash = self.state_trie.root_hash();
        account_data_to_digest(self.nonce.to_digest(), self.code_hash, state_hash)
    }

    fn acc_hash(&self) -> H256 {
        if let Some(acc_hash) = self.acc_hash.load() {
            return acc_hash;
        }

        let acc_hash = self.acc_hash_inner();
        self.acc_hash.store(Some(acc_hash));
        acc_hash
    }

    fn diff_missing_branches(&self, fork: &AccountWriteSetTrie) -> Result<AccountTrieDiff> {
        let state_trie_diff = diff_missing_branches(&self.state_trie, &fork.state_trie, true)?;

        Ok(AccountTrieDiff {
            nonce: None,
            code_hash: None,
            state_trie_diff,
        })
    }

    fn update_missing_branches(&mut self, fork: &AccountWriteSetTrie) -> Result<()> {
        debug_assert_eq!(
            self.nonce, fork.nonce,
            "Invalid nonce in AccountWriteSetTrie."
        );
        debug_assert_eq!(
            self.code_hash, fork.code_hash,
            "Invalid code hash in AccountWriteSetTrie."
        );
        self.state_trie = update_missing_branches(&self.state_trie, &fork.state_trie, true)?;
        Ok(())
    }

    fn diff_from_empty(fork: &AccountWriteSetTrie) -> AccountTrieDiff {
        let nonce = if fork.nonce.is_zero() {
            None
        } else {
            Some(fork.nonce)
        };

        let code_hash = if fork.code_hash.is_zero() {
            None
        } else {
            Some(fork.code_hash)
        };

        let state_trie_diff = PartialTrieDiff::diff_from_empty(&fork.state_trie);

        AccountTrieDiff {
            nonce,
            code_hash,
            state_trie_diff,
        }
    }

    fn create_from_empty(fork: &AccountWriteSetTrie) -> Self {
        Self::new(fork.nonce, fork.code_hash, fork.state_trie.clone())
    }

    fn apply_diff(&mut self, diff: &AccountTrieDiff, check_hash: bool) -> Result<()> {
        if let Some(nonce) = diff.nonce {
            if check_hash {
                ensure!(self.nonce == nonce, "Invalid nonce in AccountTrieDiff.")
            } else {
                debug_assert_eq!(self.nonce, nonce, "Invalid nonce in AccountTrieDiff.");
            }
        }

        if let Some(code_hash) = diff.code_hash {
            if check_hash {
                ensure!(
                    self.code_hash == code_hash,
                    "Invalid code hash in AccountTrieDiff."
                );
            } else {
                debug_assert_eq!(
                    self.code_hash, code_hash,
                    "Invalid code hash in AccountTrieDiff."
                );
            }
        }

        self.state_trie = apply_diff(&self.state_trie, &diff.state_trie_diff, check_hash)?;
        Ok(())
    }

    fn create_from_diff(diff: &AccountTrieDiff) -> Result<Self> {
        let nonce = diff.nonce.unwrap_or_default();
        let code_hash = diff.code_hash.unwrap_or_default();
        let state_trie = diff.state_trie_diff.to_standalone_trie()?;
        Ok(Self::new(nonce, code_hash, state_trie))
    }

    fn apply_writes(&mut self, writes: &AccountWriteData) -> Result<()> {
        self.reset_acc_hash();

        if let Some(nonce) = writes.nonce {
            self.nonce = nonce;
        }

        if let Some(code) = &writes.code {
            self.code_hash = code.to_digest();
        }

        if writes.reset_values {
            self.state_trie = PartialTrie::new();
        }

        let mut ctx = WritePartialTrieContext::new(self.state_trie.clone());
        for (k, v) in writes.values.iter() {
            ctx.insert_with_value(k, v)?;
        }
        self.state_trie = ctx.finish();

        Ok(())
    }

    fn prune_state_key(
        &mut self,
        key: StateKey,
        other_keys: impl Iterator<Item = StateKey>,
    ) -> Result<()> {
        self.state_trie = prune_key2(&self.state_trie, &key, other_keys)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxTrie {
    pub(crate) main_trie: PartialTrie,
    pub(crate) acc_tries: im::HashMap<Address, AccountTrie>,
}

impl TxTrie {
    pub fn diff_missing_branches(&self, fork: &TxWriteSetTrie) -> Result<TxTrieDiff> {
        let main_trie_diff = diff_missing_branches(&self.main_trie, &fork.main_trie, false)?;
        let mut acc_trie_diffs = HashMap::new();

        for (acc_addr, fork_acc_trie) in fork.acc_tries.iter() {
            match self.acc_tries.get(acc_addr) {
                Some(main_acc_trie) => {
                    let acc_diff = main_acc_trie.diff_missing_branches(fork_acc_trie)?;
                    if !acc_diff.is_empty() {
                        acc_trie_diffs.insert(*acc_addr, acc_diff);
                    }
                }
                None => {
                    if cfg!(debug_assertions) {
                        let actual = fork_acc_trie.acc_hash();
                        let expect = match self.main_trie.value_hash(acc_addr) {
                            Some(hash) => hash,
                            None => main_trie_diff
                                .value_hash(acc_addr)
                                .expect("TxTrie#diff_missing_branches: Account trie root hash not available"),
                        };
                        debug_assert_eq!(
                            actual, expect,
                            "TxTrie#diff_missing_branches: Invalid root hash in account trie (address: {})",
                            acc_addr
                        );
                    }

                    let acc_diff = AccountTrie::diff_from_empty(&fork_acc_trie);
                    if !acc_diff.is_empty() {
                        acc_trie_diffs.insert(*acc_addr, acc_diff);
                    }
                }
            }
        }

        Ok(TxTrieDiff {
            main_trie_diff,
            acc_trie_diffs,
        })
    }
}

impl TxTrieTrait for TxTrie {
    fn root_hash(&self) -> H256 {
        if cfg!(debug_assertions) {
            for (acc_addr, acc_trie) in self.acc_tries.iter() {
                debug_assert_eq!(
                    self.main_trie.value_hash(acc_addr).unwrap_or_default(),
                    acc_trie.acc_hash_inner(),
                    "TxTrie#root_hash: Hash mismatched between main trie and account trie (address: {}).",
                    acc_addr
                );
            }
        }

        self.main_trie.root_hash()
    }

    fn update_missing_branches(&mut self, fork: &TxWriteSetTrie) -> Result<()> {
        self.main_trie = update_missing_branches(&self.main_trie, &fork.main_trie, false)?;

        for (acc_addr, fork_acc_trie) in fork.acc_tries.iter() {
            match self.acc_tries.entry(*acc_addr) {
                im::hashmap::Entry::Occupied(mut o) => {
                    o.get_mut().update_missing_branches(fork_acc_trie)?;
                }
                im::hashmap::Entry::Vacant(v) => {
                    let acc_trie = AccountTrie::create_from_empty(fork_acc_trie);
                    debug_assert_eq!(
                        self.main_trie.value_hash(acc_addr),
                        Some(acc_trie.acc_hash()),
                        "TxTrie#update_missing_branches: Hash mismatched (address: {}).",
                        acc_addr
                    );
                    v.insert(acc_trie);
                }
            }
        }

        Ok(())
    }

    fn apply_diff(&mut self, diff: &TxTrieDiff, check_hash: bool) -> Result<()> {
        self.main_trie = apply_diff(&self.main_trie, &diff.main_trie_diff, check_hash)?;

        for (acc_addr, acc_trie_diff) in diff.acc_trie_diffs.iter() {
            match self.acc_tries.entry(*acc_addr) {
                im::hashmap::Entry::Occupied(mut o) => {
                    o.get_mut().apply_diff(acc_trie_diff, check_hash)?;
                }
                im::hashmap::Entry::Vacant(v) => {
                    let acc_trie = AccountTrie::create_from_diff(acc_trie_diff)?;
                    if check_hash {
                        ensure!(
                            self.main_trie.value_hash(acc_addr) == Some(acc_trie.acc_hash()),
                            "TxTrie#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    } else {
                        debug_assert_eq!(
                            self.main_trie.value_hash(acc_addr),
                            Some(acc_trie.acc_hash()),
                            "TxTrie#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    }
                    v.insert(acc_trie);
                }
            }
        }

        Ok(())
    }

    fn apply_writes(&mut self, writes: &TxWriteData) -> Result<TxStateUpdate> {
        let mut main_ctx = WritePartialTrieContext::new(self.main_trie.clone());
        for (acc_addr, acc_writes) in writes.iter() {
            let acc_trie = self.acc_tries.entry(*acc_addr).or_default();

            debug_assert_eq!(
                self.main_trie.value_hash(acc_addr).unwrap_or_default(),
                acc_trie.acc_hash_inner(),
                "TxTrie#apply_writes: Hash mismatched between main trie and account trie (address: {}).",
                acc_addr
            );

            acc_trie.apply_writes(acc_writes)?;
            let acc_hash = acc_trie.acc_hash();
            main_ctx.insert(acc_addr, acc_hash)?;
        }
        self.main_trie = main_ctx.finish();

        let mut update = TxStateUpdate::default();
        update.root = self.root_hash();
        Ok(update)
    }

    fn prune_account(
        &mut self,
        acc_addr: Address,
        other_acc_addr: impl Iterator<Item = Address>,
    ) -> Result<()> {
        let _acc_trie = self.acc_tries.remove(&acc_addr);
        debug_assert!(
            _acc_trie.is_some(),
            "TxTrie#prune_account: Account is already pruned."
        );
        self.main_trie = prune_key2(&self.main_trie, &acc_addr, other_acc_addr)?;
        Ok(())
    }

    fn prune_acc_state_key(
        &mut self,
        acc_addr: Address,
        key: StateKey,
        other_keys: impl Iterator<Item = StateKey>,
    ) -> Result<()> {
        match self.acc_tries.get_mut(&acc_addr) {
            Some(acc_trie) => acc_trie.prune_state_key(key, other_keys)?,
            None => bail!(
                "TxTrie#prune_acc_state_key: cannot find acc_trie. Address: {}",
                acc_addr
            ),
        }
        Ok(())
    }
}
