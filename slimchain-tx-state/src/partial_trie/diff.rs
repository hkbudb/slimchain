use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, Nonce},
    collections::{hash_map::Entry, HashMap},
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountTrieDiff {
    pub nonce: Option<Nonce>,
    pub code_hash: Option<H256>,
    pub state_trie_diff: PartialTrieDiff,
}

impl AccountTrieDiff {
    pub fn is_empty(&self) -> bool {
        self.nonce.is_none() && self.code_hash.is_none() && self.state_trie_diff.is_empty()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxTrieDiff {
    pub main_trie_diff: PartialTrieDiff,
    pub acc_trie_diffs: HashMap<Address, AccountTrieDiff>,
}

pub fn merge_acc_trie_diff(lhs: &AccountTrieDiff, rhs: &AccountTrieDiff) -> AccountTrieDiff {
    debug_assert_eq!(lhs.nonce, rhs.nonce);
    debug_assert_eq!(lhs.code_hash, rhs.code_hash);
    let state_trie_diff = merge_diff(&lhs.state_trie_diff, &rhs.state_trie_diff);
    AccountTrieDiff {
        nonce: lhs.nonce,
        code_hash: lhs.code_hash,
        state_trie_diff,
    }
}

pub fn merge_tx_trie_diff(lhs: &TxTrieDiff, rhs: &TxTrieDiff) -> TxTrieDiff {
    let mut acc_trie_diffs = lhs.acc_trie_diffs.clone();

    for (addr, diff) in rhs.acc_trie_diffs.iter() {
        match acc_trie_diffs.entry(*addr) {
            Entry::Occupied(mut o) => {
                let merged_diff = merge_acc_trie_diff(o.get(), diff);
                *o.get_mut() = merged_diff;
            }
            Entry::Vacant(v) => {
                v.insert(diff.clone());
            }
        }
    }

    TxTrieDiff {
        main_trie_diff: merge_diff(&lhs.main_trie_diff, &rhs.main_trie_diff),
        acc_trie_diffs,
    }
}
