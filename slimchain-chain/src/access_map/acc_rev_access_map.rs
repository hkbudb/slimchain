use super::BlockHeightList;
use serde::{Deserialize, Serialize};
use slimchain_common::basic::{BlockHeight, StateKey};

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadRevAccessItem {
    nonce: BlockHeightList,
    code: BlockHeightList,
    values: im::HashMap<StateKey, BlockHeightList>,
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
            im::hashmap::Entry::Occupied(mut o) => {
                o.get_mut().remove_block_height(block_height);
                if o.get().is_empty() {
                    o.remove();
                }
            }
            im::hashmap::Entry::Vacant(_) => unreachable!(),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriteRevAccessItem {
    nonce: BlockHeightList,
    code: BlockHeightList,
    reset_values: BlockHeightList,
    values: im::HashMap<StateKey, BlockHeightList>,
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

    pub fn remove_nonce(&mut self, block_height: BlockHeight) -> bool {
        self.nonce.remove_block_height(block_height);
        self.nonce.is_empty()
    }

    pub fn remove_code(&mut self, block_height: BlockHeight) -> bool {
        self.code.remove_block_height(block_height);
        self.code.is_empty()
    }

    pub fn remove_reset_values(&mut self, block_height: BlockHeight) -> bool {
        self.reset_values.remove_block_height(block_height);
        self.reset_values.is_empty()
    }

    pub fn remove_value(&mut self, key: StateKey, block_height: BlockHeight) -> bool {
        let mut should_prune = false;
        match self.values.entry(key) {
            im::hashmap::Entry::Occupied(mut o) => {
                o.get_mut().remove_block_height(block_height);
                if o.get().is_empty() {
                    // only prune the state key if it is not reset in a later block
                    if self.reset_values.is_empty() {
                        should_prune = true;
                    }

                    o.remove();
                }
            }
            im::hashmap::Entry::Vacant(_) => unreachable!(),
        }

        should_prune
    }
}
