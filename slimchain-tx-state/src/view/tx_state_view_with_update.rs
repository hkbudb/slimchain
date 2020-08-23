use crate::{TxStateUpdate, TxStateView};
use slimchain_common::{
    basic::{AccountData, Address, StateValue, H256},
    error::Result,
};
use slimchain_merkle_trie::storage::TrieNode;

pub struct TxStateViewWithUpdate<'a, View: TxStateView + ?Sized> {
    view: &'a View,
    update: &'a TxStateUpdate,
}

impl<'a, View: TxStateView + ?Sized> TxStateViewWithUpdate<'a, View> {
    pub fn new(view: &'a View, update: &'a TxStateUpdate) -> Self {
        Self { view, update }
    }
}

impl<'a, View: TxStateView + ?Sized> TxStateView for TxStateViewWithUpdate<'a, View> {
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>> {
        if let Some(node) = self.update.acc_nodes.get(&node_address) {
            return Ok(node.clone());
        }

        self.view.account_trie_node(node_address)
    }

    fn state_trie_node(
        &self,
        acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>> {
        if let Some(node) = self
            .update
            .state_nodes
            .get(&acc_address)
            .and_then(|nodes| nodes.get(&node_address))
        {
            return Ok(node.clone());
        }

        self.view.state_trie_node(acc_address, node_address)
    }
}
