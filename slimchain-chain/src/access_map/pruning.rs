use slimchain_common::{
    basic::{Address, StateKey},
    collections::{HashMap, HashSet},
    error::Result,
};
use slimchain_tx_state::TxTrieTrait;

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct PruningData {
    pub(crate) nonce: HashSet<Address>,
    pub(crate) code: HashSet<Address>,
    pub(crate) reset_values: HashSet<Address>,
    pub(crate) values: HashMap<Address, HashSet<StateKey>>,
}

impl PruningData {
    pub fn add_nonce(&mut self, acc_addr: Address) {
        self.nonce.insert(acc_addr);
    }

    pub fn add_code(&mut self, acc_addr: Address) {
        self.code.insert(acc_addr);
    }

    pub fn add_reset_values(&mut self, acc_addr: Address) {
        self.reset_values.insert(acc_addr);
    }

    pub fn add_value(&mut self, acc_addr: Address, key: StateKey) {
        self.values.entry(acc_addr).or_default().insert(key);
    }

    pub fn prune_tx_trie(&self, tx_trie: &mut impl TxTrieTrait) -> Result<()> {
        for &acc_addr in &self.nonce {
            tx_trie.prune_acc_nonce(acc_addr)?;
        }

        for &acc_addr in &self.code {
            tx_trie.prune_acc_code(acc_addr)?;
        }

        for &acc_addr in &self.reset_values {
            tx_trie.prune_acc_reset_values(acc_addr)?;
        }

        for (&acc_addr, keys) in &self.values {
            tx_trie.prune_acc_state_keys(acc_addr, keys.iter().copied())?;
        }

        Ok(())
    }
}
