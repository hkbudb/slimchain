use crate::view::{
    trie_view_sync::{AccountTrieView, StateTrieView},
    TxStateView,
};
use alloc::{format, sync::Arc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, H256},
    collections::HashMap,
    digest::Digestible,
    error::{ensure, Result},
    rw_set::TxWriteData,
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountWriteSetPartialTrie {
    pub nonce: Nonce,
    pub code_hash: H256,
    pub state_partial_trie: PartialTrie,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxWriteSetPartialTrie {
    pub main_partial_trie: PartialTrie,
    pub acc_partial_tries: HashMap<Address, AccountWriteSetPartialTrie>,
}

impl TxWriteSetPartialTrie {
    pub fn new(
        state_view: Arc<dyn TxStateView + Sync + Send>,
        root_address: H256,
        writes: &TxWriteData,
    ) -> Result<Self> {
        let mut acc_partial_tries: HashMap<Address, AccountWriteSetPartialTrie> = HashMap::new();
        let mut main_read_ctx =
            ReadTrieContext::new(AccountTrieView::new(Arc::clone(&state_view)), root_address);

        for (acc_address, acc_write) in writes.iter() {
            let acc_data = main_read_ctx.read(acc_address)?;
            let acc_state_root = acc_data.map(|acc| acc.acc_state_root).unwrap_or_default();
            let state_partial_trie = if acc_write.reset_values {
                PartialTrie::from_root_hash(acc_state_root)
            } else {
                let mut value_read_ctx = ReadTrieContext::new(
                    StateTrieView::new(Arc::clone(&state_view), *acc_address),
                    acc_state_root,
                );

                for key in acc_write.values.keys() {
                    value_read_ctx.read(key)?;
                }

                value_read_ctx.into_proof().into()
            };

            let acc_proof = AccountWriteSetPartialTrie {
                nonce: acc_data.map(|acc| acc.nonce).unwrap_or_default(),
                code_hash: acc_data.map(|acc| acc.code.to_digest()).unwrap_or_default(),
                state_partial_trie,
            };

            acc_partial_tries.insert(*acc_address, acc_proof);
        }

        Ok(Self {
            main_partial_trie: main_read_ctx.into_proof().into(),
            acc_partial_tries,
        })
    }

    pub fn verify(&self, state_root: H256) -> Result<()> {
        for (acc_address, acc_partial_trie) in self.acc_partial_tries.iter() {
            let acc_hash = account_data_to_digest(
                acc_partial_trie.nonce.to_digest(),
                acc_partial_trie.code_hash,
                acc_partial_trie.state_partial_trie.root_hash(),
            );

            let main_partial_trie_acc_hash = self.main_partial_trie.value_hash(acc_address);

            ensure!(
                main_partial_trie_acc_hash == Some(acc_hash),
                "TxWriteSetPartialTrie: Invalid account hash (address: {}, expect: {:?}, actual: {:?}).",
                acc_address,
                main_partial_trie_acc_hash,
                Some(acc_hash)
            );
        }

        let main_partial_trie_root = self.main_partial_trie.root_hash();
        ensure!(
            main_partial_trie_root == state_root,
            "TxWriteSetPartialTrie: Invalid state root (expect: {}, actual: {}).",
            state_root,
            main_partial_trie_root
        );

        Ok(())
    }
}
