use super::BlockHeightList;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{BlockHeight, StateKey},
    rw_set::{AccountReadSet, AccountWriteData},
};

pub trait RevAccessItem {
    fn nonce_conflicts_with(&self, block_height: BlockHeight) -> bool;
    fn code_conflicts_with(&self, block_height: BlockHeight) -> bool;
    fn reset_values_conflict_with(&self, block_height: BlockHeight) -> bool;
    fn value_conflicts_with(&self, block_height: BlockHeight, key: StateKey) -> bool;

    fn has_conflict_in_read_set(
        &self,
        block_height: BlockHeight,
        acc_read: &AccountReadSet,
    ) -> bool {
        if acc_read.get_nonce() && self.nonce_conflicts_with(block_height) {
            return true;
        }

        if acc_read.get_code() && self.code_conflicts_with(block_height) {
            return true;
        }

        for &key in acc_read.value_iter() {
            if self.value_conflicts_with(block_height, key) {
                return true;
            }
        }

        false
    }

    fn has_conflict_in_write_set(
        &self,
        block_height: BlockHeight,
        acc_write: &AccountWriteData,
    ) -> bool {
        if acc_write.has_nonce() && self.nonce_conflicts_with(block_height) {
            return true;
        }

        if acc_write.has_code() && self.code_conflicts_with(block_height) {
            return true;
        }

        if acc_write.has_reset_values() && self.reset_values_conflict_with(block_height) {
            return true;
        }

        for &key in acc_write.value_keys() {
            if self.value_conflicts_with(block_height, key) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadRevAccessItem {
    nonce: BlockHeightList,
    code: BlockHeightList,
    values: imbl::HashMap<StateKey, BlockHeightList>,
}

impl ReadRevAccessItem {
    pub fn is_empty(&self) -> bool {
        self.nonce.is_empty() && self.code.is_empty() && self.values.is_empty()
    }

    pub fn add_nonce(&mut self, block_height: BlockHeight) {
        self.nonce.add_block_height(block_height);
    }

    pub fn add_code(&mut self, block_height: BlockHeight) {
        self.code.add_block_height(block_height);
    }

    pub fn add_value(&mut self, key: StateKey, block_height: BlockHeight) {
        self.values
            .entry(key)
            .or_default()
            .add_block_height(block_height);
    }

    pub fn remove_nonce(&mut self, block_height: BlockHeight) {
        self.nonce.remove_block_height(block_height);
    }

    pub fn remove_code(&mut self, block_height: BlockHeight) {
        self.code.remove_block_height(block_height);
    }

    pub fn remove_value(&mut self, key: StateKey, block_height: BlockHeight) {
        match self.values.entry(key) {
            imbl::hashmap::Entry::Occupied(mut o) => {
                o.get_mut().remove_block_height(block_height);
                if o.get().is_empty() {
                    o.remove();
                }
            }
            imbl::hashmap::Entry::Vacant(_) => unreachable!(),
        }
    }
}

impl RevAccessItem for ReadRevAccessItem {
    fn nonce_conflicts_with(&self, block_height: BlockHeight) -> bool {
        self.nonce.conflicts_with(block_height)
    }

    fn code_conflicts_with(&self, block_height: BlockHeight) -> bool {
        self.code.conflicts_with(block_height)
    }

    fn reset_values_conflict_with(&self, block_height: BlockHeight) -> bool {
        self.values.values().any(|l| l.conflicts_with(block_height))
    }

    fn value_conflicts_with(&self, block_height: BlockHeight, key: StateKey) -> bool {
        self.values
            .get(&key)
            .map_or(false, |l| l.conflicts_with(block_height))
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriteRevAccessItem {
    nonce: BlockHeightList,
    code: BlockHeightList,
    reset_values: BlockHeightList,
    values: imbl::OrdMap<StateKey, BlockHeightList>,
}

impl WriteRevAccessItem {
    pub fn is_empty(&self) -> bool {
        self.nonce.is_empty()
            && self.code.is_empty()
            && self.reset_values.is_empty()
            && self.values.is_empty()
    }

    pub fn add_nonce(&mut self, block_height: BlockHeight) {
        self.nonce.add_block_height(block_height);
    }

    pub fn add_code(&mut self, block_height: BlockHeight) {
        self.code.add_block_height(block_height);
    }

    pub fn add_reset_values(&mut self, block_height: BlockHeight) {
        self.reset_values.add_block_height(block_height);
    }

    pub fn add_value(&mut self, key: StateKey, block_height: BlockHeight) {
        self.values
            .entry(key)
            .or_default()
            .add_block_height(block_height);
    }

    pub fn remove_nonce(&mut self, block_height: BlockHeight) {
        self.nonce.remove_block_height(block_height);
    }

    pub fn remove_code(&mut self, block_height: BlockHeight) {
        self.code.remove_block_height(block_height);
    }

    pub fn remove_reset_values(&mut self, block_height: BlockHeight) {
        self.reset_values.remove_block_height(block_height);
    }

    pub fn remove_value(&mut self, key: StateKey, block_height: BlockHeight) -> bool {
        let mut should_prune = false;
        match self.values.entry(key) {
            imbl::ordmap::Entry::Occupied(mut o) => {
                o.get_mut().remove_block_height(block_height);
                if o.get().is_empty() {
                    // only prune the state key if it is not reset in a later block
                    if self.reset_values.is_empty() {
                        should_prune = true;
                    }

                    o.remove();
                }
            }
            imbl::ordmap::Entry::Vacant(_) => unreachable!(),
        }

        should_prune
    }

    pub(crate) fn state_values(&self) -> &'_ imbl::OrdMap<StateKey, BlockHeightList> {
        &self.values
    }
}

impl RevAccessItem for WriteRevAccessItem {
    fn nonce_conflicts_with(&self, block_height: BlockHeight) -> bool {
        self.nonce.conflicts_with(block_height)
    }

    fn code_conflicts_with(&self, block_height: BlockHeight) -> bool {
        self.code.conflicts_with(block_height)
    }

    fn reset_values_conflict_with(&self, block_height: BlockHeight) -> bool {
        if self.reset_values.conflicts_with(block_height) {
            return true;
        }

        self.values.values().any(|l| l.conflicts_with(block_height))
    }

    fn value_conflicts_with(&self, block_height: BlockHeight, key: StateKey) -> bool {
        if self.reset_values.conflicts_with(block_height) {
            return true;
        }

        self.values
            .get(&key)
            .map_or(false, |l| l.conflicts_with(block_height))
    }
}
