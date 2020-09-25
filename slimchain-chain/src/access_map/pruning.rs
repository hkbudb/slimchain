use super::AccessMap;
use slimchain_common::{
    basic::{Address, StateKey},
    collections::{HashMap, HashSet},
    error::{Context as _, Result},
};
use slimchain_tx_state::TxTrieTrait;

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct PruningData {
    pub(crate) accounts: HashSet<Address>,
    pub(crate) values: HashMap<Address, HashSet<StateKey>>,
}

impl PruningData {
    pub fn add_account(&mut self, acc_addr: Address) {
        self.values.remove(&acc_addr);
        self.accounts.insert(acc_addr);
    }

    pub fn add_value(&mut self, acc_addr: Address, key: StateKey) {
        self.values.entry(acc_addr).or_default().insert(key);
    }

    pub fn prune_tx_trie(
        self,
        access_map: &AccessMap,
        tx_trie: &mut impl TxTrieTrait,
    ) -> Result<()> {
        let write_rev_map = access_map.write_rev_map();

        for acc_addr in self.accounts {
            tx_trie.prune_account(acc_addr, write_rev_map.keys().copied())?;
        }

        for (acc_addr, keys) in self.values {
            let other_keys = write_rev_map
                .get(&acc_addr)
                .context("PruningData#prune_tx_trie: write_rev_access map cannot be found")?
                .value_keys();

            for key in keys {
                tx_trie.prune_acc_state_key(acc_addr, key, other_keys.iter().copied())?;
            }
        }

        Ok(())
    }
}
