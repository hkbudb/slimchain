use super::TxStateView;
use slimchain_common::{
    basic::{AccountData, Address, StateValue, H256},
    error::Result,
};
use slimchain_merkle_trie::storage::{NodeLoader, TrieNode};

pub struct AccountTrieView<'a, View: TxStateView> {
    pub state_view: &'a View,
}

impl<'a, View: TxStateView> AccountTrieView<'a, View> {
    pub fn new(state_view: &'a View) -> Self {
        Self { state_view }
    }
}

impl<'a, View: TxStateView> NodeLoader<AccountData> for AccountTrieView<'a, View> {
    fn load_node(&self, address: H256) -> Result<TrieNode<AccountData>> {
        self.state_view.account_trie_node(address)
    }
}

pub struct StateTrieView<'a, View: TxStateView> {
    pub state_view: &'a View,
    pub acc_address: Address,
}

impl<'a, View: TxStateView> StateTrieView<'a, View> {
    pub fn new(state_view: &'a View, acc_address: Address) -> Self {
        Self {
            state_view,
            acc_address,
        }
    }
}

impl<'a, View: TxStateView> NodeLoader<StateValue> for StateTrieView<'a, View> {
    fn load_node(&self, address: H256) -> Result<TrieNode<StateValue>> {
        self.state_view.state_trie_node(self.acc_address, address)
    }
}
