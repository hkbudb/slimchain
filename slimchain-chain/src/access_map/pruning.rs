use super::AccessMap;
use slimchain_common::{
    basic::{Address, StateKey},
    collections::{HashMap, HashSet},
    error::{Context as _, Result},
};
use slimchain_merkle_trie::prelude::*;
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
            let acc_addr_nibbles = acc_addr.as_nibbles();
            let prev_common_len = write_rev_map
                .range(..acc_addr)
                .next_back()
                .map(|(k, _v)| acc_addr_nibbles.common_prefix_len(&k));
            let next_common_len = write_rev_map
                .range(acc_addr..)
                .next()
                .map(|(k, _v)| acc_addr_nibbles.common_prefix_len(&k));
            let max_common_len = [prev_common_len, next_common_len]
                .iter()
                .copied()
                .filter_map(|x| x)
                .max()
                .unwrap_or(0);
            let kept_prefix_len = if max_common_len == 0 {
                1
            } else {
                max_common_len
            };
            tx_trie.prune_account(acc_addr, kept_prefix_len)?;
        }

        for (acc_addr, keys) in self.values {
            let other_state_values = write_rev_map
                .get(&acc_addr)
                .context("PruningData#prune_tx_trie: write_rev_access map cannot be found")?
                .state_values();

            for key in keys {
                let key_nibbles = key.as_nibbles();
                let prev_common_len = other_state_values
                    .range(..key)
                    .next_back()
                    .map(|(k, _v)| key_nibbles.common_prefix_len(&k));
                let next_common_len = other_state_values
                    .range(key..)
                    .next()
                    .map(|(k, _v)| key_nibbles.common_prefix_len(&k));
                let max_common_len = [prev_common_len, next_common_len]
                    .iter()
                    .copied()
                    .filter_map(|x| x)
                    .max()
                    .unwrap_or(0);
                let kept_prefix_len = if max_common_len == 0 {
                    1
                } else {
                    max_common_len
                };
                tx_trie.prune_acc_state_key(acc_addr, key, kept_prefix_len)?;
            }
        }

        Ok(())
    }
}
