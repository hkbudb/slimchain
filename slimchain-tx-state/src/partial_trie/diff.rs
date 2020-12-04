use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, Nonce},
    collections::{hash_map::Entry, HashMap},
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AccountTrieDiff {
    pub(crate) nonce: Option<Nonce>,
    pub(crate) code_hash: Option<H256>,
    pub(crate) state_trie_diff: PartialTrieDiff,
}

impl AccountTrieDiff {
    pub(crate) fn is_empty(&self) -> bool {
        self.nonce.is_none() && self.code_hash.is_none() && self.state_trie_diff.is_empty()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxTrieDiff {
    pub(crate) main_trie_diff: PartialTrieDiff,
    pub(crate) acc_trie_diffs: HashMap<Address, AccountTrieDiff>,
}

impl TxTrieDiff {
    #[cfg(feature = "draw")]
    pub fn to_graph(
        &self,
        name: impl alloc::string::ToString,
    ) -> slimchain_merkle_trie::draw::MultiGraph {
        use alloc::format;
        use slimchain_merkle_trie::draw::*;

        let name = name.to_string();
        let mut graph = MultiGraph::new(name.clone());
        let mut main_trie_diff_graph = MultiGraph::from_partial_trie_diff(
            format!("{}_main_trie_diff", name),
            &self.main_trie_diff,
        );
        main_trie_diff_graph.set_label("main_trie_diff");
        graph.add_sub_multi_graph(&main_trie_diff_graph);

        for (i, (acc_addr, acc_trie_diff)) in self.acc_trie_diffs.iter().enumerate() {
            let mut acc_trie_diff_graph = MultiGraph::from_partial_trie_diff(
                format!("{}_acc_trie_diff_{}", name, i),
                &acc_trie_diff.state_trie_diff,
            );
            acc_trie_diff_graph.set_label(format!(
                "addr = {}\nnonce = {:?}\ncode_hash = {:?}",
                acc_addr, acc_trie_diff.nonce, acc_trie_diff.code_hash,
            ));
            graph.add_sub_multi_graph(&acc_trie_diff_graph);
        }

        graph
    }

    #[cfg(feature = "draw")]
    pub fn draw(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        use slimchain_merkle_trie::draw::*;
        let graph = self.to_graph("tx_trie_diff");
        draw_dot(graph.to_dot(false), path)
    }
}

fn merge_acc_trie_diff(lhs: &AccountTrieDiff, rhs: &AccountTrieDiff) -> AccountTrieDiff {
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
