use super::{AccountTrieDiff, AccountWriteSetPartialTrie, TxTrieDiff, TxWriteSetPartialTrie};
use alloc::format;
use core::cell::Cell;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, StateKey, H256},
    collections::HashMap,
    digest::Digestible,
    error::{bail, ensure, Result},
    rw_set::{AccessFlags, AccountWriteData, TxWriteData},
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountTrie {
    pub nonce: Nonce,
    pub code_hash: H256,
    pub state_trie: PartialTrie,
    pub access_flags: AccessFlags,
    #[serde(skip)]
    root_hash: Cell<Option<H256>>,
}

impl AccountTrie {
    fn reset_root_hash(&mut self) {
        self.root_hash.set(None);
    }

    pub fn new(
        nonce: Nonce,
        code_hash: H256,
        state_trie: PartialTrie,
        access_flags: AccessFlags,
    ) -> Self {
        Self {
            nonce,
            code_hash,
            state_trie,
            access_flags,
            root_hash: Cell::new(None),
        }
    }

    fn root_hash_inner(&self) -> H256 {
        let state_hash = self.state_trie.root_hash();
        account_data_to_digest(self.nonce.to_digest(), self.code_hash, state_hash)
    }

    pub fn root_hash(&self) -> H256 {
        if let Some(root_hash) = self.root_hash.get() {
            return root_hash;
        }

        let root_hash = self.root_hash_inner();
        self.root_hash.set(Some(root_hash));
        root_hash
    }

    pub fn diff_missing_branches(
        &self,
        fork: &AccountWriteSetPartialTrie,
    ) -> Result<AccountTrieDiff> {
        let state_trie_diff = diff_missing_branches(&self.state_trie, &fork.state_trie, true)?;

        Ok(AccountTrieDiff {
            nonce: None,
            code_hash: None,
            state_trie_diff,
        })
    }

    pub fn diff_from_empty(fork: &AccountWriteSetPartialTrie) -> AccountTrieDiff {
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

    pub fn apply_diff(&mut self, diff: &AccountTrieDiff, check_hash: bool) -> Result<()> {
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

    pub fn create_from_diff(diff: &AccountTrieDiff) -> Result<Self> {
        let nonce = diff.nonce.unwrap_or_default();
        let code_hash = diff.code_hash.unwrap_or_default();
        let state_trie = diff.state_trie_diff.to_standalone_trie()?;
        Ok(Self::new(
            nonce,
            code_hash,
            state_trie,
            AccessFlags::empty(),
        ))
    }

    pub fn apply_writes(&mut self, writes: &AccountWriteData) -> Result<()> {
        self.reset_root_hash();

        if let Some(nonce) = writes.nonce {
            self.nonce = nonce;
            self.access_flags.set_nonce(true);
        }

        if let Some(code) = &writes.code {
            self.code_hash = code.to_digest();
            self.access_flags.set_code(true);
        }

        if writes.reset_values {
            self.state_trie = PartialTrie::new();
            self.access_flags.set_state(true);
        }

        if !writes.values.is_empty() {
            self.access_flags.set_state(true);
        }

        let mut ctx = WritePartialTrieContext::new(self.state_trie.clone());
        for (k, v) in writes.values.iter() {
            ctx.insert_with_value(k, v)?;
        }
        self.state_trie = ctx.finish();

        Ok(())
    }

    pub fn can_be_pruned(&self) -> bool {
        self.access_flags.is_empty()
    }

    pub fn prune_nonce(&mut self) {
        self.access_flags.set_nonce(false);
    }

    pub fn prune_code(&mut self) {
        self.access_flags.set_code(false);
    }

    pub fn prune_state_key(&mut self, key: StateKey) -> Result<()> {
        self.state_trie = prune_unused_key(&self.state_trie, &key)?;

        if self.state_trie.can_be_pruned() {
            self.access_flags.set_state(false);
        }

        Ok(())
    }

    pub fn prune_state_keys(&mut self, keys: impl Iterator<Item = StateKey>) -> Result<()> {
        self.state_trie = prune_unused_keys(&self.state_trie, keys)?;

        if self.state_trie.can_be_pruned() {
            self.access_flags.set_state(false);
        }

        Ok(())
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxTrie {
    pub main_trie: PartialTrie,
    pub acc_tries: im::HashMap<Address, AccountTrie>,
}

impl TxTrie {
    pub fn root_hash(&self) -> H256 {
        if cfg!(debug_assertions) {
            for (acc_addr, acc_trie) in self.acc_tries.iter() {
                debug_assert_eq!(
                    self.main_trie.value_hash(acc_addr),
                    Some(acc_trie.root_hash_inner()),
                    "TxTrie#root_hash: Hash mismatched between main trie and account trie (address: {}).",
                    acc_addr
                );
            }
        }

        self.main_trie.root_hash()
    }

    pub fn diff_missing_branches(&self, fork: &TxWriteSetPartialTrie) -> Result<TxTrieDiff> {
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
                        let actual = fork_acc_trie.root_hash();
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

    pub fn apply_diff(&mut self, diff: &TxTrieDiff, check_hash: bool) -> Result<()> {
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
                            self.main_trie.value_hash(acc_addr) == Some(acc_trie.root_hash()),
                            "TxTrie#apply_diff: Hash mismatched (address: {}).",
                            acc_addr
                        );
                    } else {
                        debug_assert_eq!(
                            self.main_trie.value_hash(acc_addr),
                            Some(acc_trie.root_hash()),
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

    pub fn apply_writes(&mut self, writes: &TxWriteData) -> Result<()> {
        let mut main_ctx = WritePartialTrieContext::new(self.main_trie.clone());
        for (acc_addr, acc_writes) in writes.iter() {
            let acc_trie = self.acc_tries.entry(*acc_addr).or_default();

            debug_assert_eq!(
                self.main_trie.value_hash(acc_addr),
                Some(acc_trie.root_hash_inner()),
                "TxTrie#apply_writes: Hash mismatched between main trie and account trie (address: {}).",
                acc_addr
            );

            acc_trie.apply_writes(acc_writes)?;
            let acc_hash = acc_trie.root_hash();
            main_ctx.insert(acc_addr, acc_hash)?;
        }
        self.main_trie = main_ctx.finish();

        Ok(())
    }

    fn prune_helper(
        &mut self,
        acc_addr: Address,
        callback: impl FnOnce(&mut AccountTrie) -> Result<()>,
    ) -> Result<()> {
        let mut entry = match self.acc_tries.entry(acc_addr) {
            im::hashmap::Entry::Occupied(o) => o,
            im::hashmap::Entry::Vacant(_) => {
                bail!("TxTrie#prune_helper: Account is already pruned");
            }
        };

        callback(entry.get_mut())?;

        if entry.get().can_be_pruned() {
            entry.remove();

            self.main_trie = prune_unused_key(&self.main_trie, &acc_addr)?;
        }

        Ok(())
    }

    pub fn prune_acc_nonce(&mut self, acc_addr: Address) -> Result<()> {
        self.prune_helper(acc_addr, |acc_trie| {
            acc_trie.prune_nonce();
            Ok(())
        })
    }

    pub fn prune_acc_code(&mut self, acc_addr: Address) -> Result<()> {
        self.prune_helper(acc_addr, |acc_trie| {
            acc_trie.prune_code();
            Ok(())
        })
    }

    pub fn prune_acc_state_key(&mut self, acc_addr: Address, key: StateKey) -> Result<()> {
        self.prune_helper(acc_addr, |acc_trie| acc_trie.prune_state_key(key))
    }

    pub fn prune_acc_state_keys(
        &mut self,
        acc_addr: Address,
        keys: impl Iterator<Item = StateKey>,
    ) -> Result<()> {
        self.prune_helper(acc_addr, move |acc_trie| acc_trie.prune_state_keys(keys))
    }
}
