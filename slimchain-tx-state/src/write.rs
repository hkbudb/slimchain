use crate::view::{
    trie_view::{AccountTrieView, StateTrieView},
    TxStateView,
};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{AccountData, Address, StateKey, StateValue, H256},
    collections::HashMap,
    error::Result,
    rw_set::TxWriteData,
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TxStateUpdate {
    pub root: H256,
    pub acc_nodes: HashMap<H256, TrieNode<AccountData>>,
    pub state_nodes: HashMap<Address, HashMap<H256, TrieNode<StateValue>>>,
}

impl TxStateUpdate {
    pub fn merge(&mut self, other: TxStateUpdate) {
        self.root = other.root;
        self.acc_nodes.extend(other.acc_nodes.into_iter());
        for (acc_addr, nodes) in other.state_nodes {
            self.state_nodes
                .entry(acc_addr)
                .or_default()
                .extend(nodes.into_iter());
        }
    }
}

pub fn update_tx_state(
    view: &impl TxStateView,
    old_root: H256,
    writes: &TxWriteData,
) -> Result<TxStateUpdate> {
    let mut updates = TxStateUpdate::default();
    let mut acc_write_ctx =
        WriteTrieContext::<Address, _, _>::new(AccountTrieView::new(view), old_root);

    for (&acc_addr, acc_data) in writes.iter() {
        let old_acc_data =
            read_trie_without_proof(&AccountTrieView::new(view), old_root, &acc_addr)?
                .unwrap_or_default();

        let acc_state_root = if acc_data.reset_values {
            H256::zero()
        } else {
            old_acc_data.acc_state_root
        };

        let mut state_write_ctx = WriteTrieContext::<StateKey, _, _>::new(
            StateTrieView::new(view, acc_addr),
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
    }

    let acc_apply = acc_write_ctx.changes();
    updates.root = acc_apply.root;
    updates.acc_nodes.extend(acc_apply.nodes.into_iter());

    Ok(updates)
}
