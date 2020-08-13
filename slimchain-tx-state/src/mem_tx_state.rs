use crate::view::TxStateView;
#[cfg(feature = "write")]
use crate::write::TxStateUpdate;
use slimchain_common::{
    basic::{AccountData, Address, StateValue, H256},
    collections::HashMap,
    error::{Context as _, Result},
    rw_set::TxWriteData,
};
use slimchain_merkle_trie::prelude::*;
use std::sync::{Arc, RwLock, RwLockReadGuard};

#[derive(Debug, Default, Clone)]
pub struct MemTxStateInternal {
    pub state_root: H256,
    pub acc_nodes: HashMap<H256, TrieNode<AccountData>>,
    pub state_nodes: HashMap<Address, HashMap<H256, TrieNode<StateValue>>>,
}

pub struct MemTxState(RwLock<MemTxStateInternal>);

impl MemTxState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(RwLock::new(MemTxStateInternal::default())))
    }

    pub fn state_view(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(&self)
    }

    pub fn state_root(&self) -> H256 {
        let internal = self.0.read().unwrap();
        internal.state_root
    }

    pub fn get_internal(&self) -> RwLockReadGuard<MemTxStateInternal> {
        self.0.read().expect("Failed to lock MemTxState.")
    }

    #[cfg(feature = "write")]
    pub fn apply_update(self: &mut Arc<Self>, update: TxStateUpdate) -> Result<()> {
        let TxStateUpdate {
            root,
            acc_nodes,
            state_nodes,
        } = update;
        let mut internal = self.0.write().expect("Failed to lock MemTxState.");
        internal.state_root = root;
        internal.acc_nodes.extend(acc_nodes.into_iter());
        for (acc_addr, nodes) in state_nodes {
            internal
                .state_nodes
                .entry(acc_addr)
                .or_default()
                .extend(nodes.into_iter());
        }
        Ok(())
    }

    #[cfg(feature = "write")]
    pub fn apply_writes(self: &mut Arc<Self>, writes: &TxWriteData) -> Result<()> {
        let update = crate::write::update_tx_state(&self.state_view(), self.state_root(), writes)?;
        self.apply_update(update)
    }
}

impl TxStateView for MemTxState {
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>> {
        let internal = self.get_internal();
        internal
            .acc_nodes
            .get(&node_address)
            .cloned()
            .context("Unknown node")
    }

    fn state_trie_node(
        &self,
        acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>> {
        let internal = self.get_internal();
        internal
            .state_nodes
            .get(&acc_address)
            .context("Unknown acc_address")?
            .get(&node_address)
            .cloned()
            .context("Unknown node")
    }
}

impl TxStateView for Arc<MemTxState> {
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>> {
        self.as_ref().account_trie_node(node_address)
    }

    fn state_trie_node(
        &self,
        acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>> {
        self.as_ref().state_trie_node(acc_address, node_address)
    }
}
