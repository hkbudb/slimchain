use super::TxStateView;
use alloc::sync::Arc;
use slimchain_common::{
    basic::{AccountData, Address, StateValue, H256},
    error::Result,
};
use slimchain_merkle_trie::storage::{NodeLoader, TrieNode};

pub struct AccountTrieView {
    pub state_view: Arc<dyn TxStateView + Sync + Send>,
}

impl AccountTrieView {
    pub fn new(state_view: Arc<dyn TxStateView + Sync + Send>) -> Self {
        Self { state_view }
    }
}

impl NodeLoader<AccountData> for AccountTrieView {
    fn load_node(&self, address: H256) -> Result<TrieNode<AccountData>> {
        self.state_view.account_trie_node(address)
    }
}

pub struct StateTrieView {
    pub state_view: Arc<dyn TxStateView + Sync + Send>,
    pub acc_address: Address,
}

impl StateTrieView {
    pub fn new(state_view: Arc<dyn TxStateView + Sync + Send>, acc_address: Address) -> Self {
        Self {
            state_view,
            acc_address,
        }
    }
}

impl NodeLoader<StateValue> for StateTrieView {
    fn load_node(&self, address: H256) -> Result<TrieNode<StateValue>> {
        self.state_view.state_trie_node(self.acc_address, address)
    }
}
