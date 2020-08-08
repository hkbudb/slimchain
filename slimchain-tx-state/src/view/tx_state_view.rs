use slimchain_common::{
    basic::{AccountData, Address, StateValue, H256},
    error::Result,
};
use slimchain_merkle_trie::storage::TrieNode;

pub trait TxStateView {
    fn account_trie_node(&self, node_address: H256) -> Result<TrieNode<AccountData>>;

    fn state_trie_node(
        &self,
        acc_address: Address,
        node_address: H256,
    ) -> Result<TrieNode<StateValue>>;
}
