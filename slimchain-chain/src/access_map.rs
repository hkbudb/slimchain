use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, BlockDistance, BlockHeight},
    rw_set::{TxReadSet, TxWriteData},
};

pub mod block_height_list;
pub use block_height_list::*;

pub mod acc_access_map;
pub use acc_access_map::*;

pub mod acc_rev_access_map;
pub use acc_rev_access_map::*;

pub mod pruning;
pub use pruning::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccessMap {
    max_blocks: usize,
    block_height: BlockHeight,
    read_map: im::Vector<ReadAccessItem>,
    write_map: im::Vector<WriteAccessItem>,
    read_rev_map: im::HashMap<Address, ReadRevAccessItem>,
    write_rev_map: im::HashMap<Address, WriteRevAccessItem>,
}

impl AccessMap {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            max_blocks,
            block_height: 0.into(),
            read_map: im::vector![ReadAccessItem::default()],
            write_map: im::vector![WriteAccessItem::default()],
            read_rev_map: im::HashMap::new(),
            write_rev_map: im::HashMap::new(),
        }
    }

    pub fn latest_block_height(&self) -> BlockHeight {
        self.block_height
    }

    pub fn oldest_block_height(&self) -> BlockHeight {
        let dist = BlockDistance::from(self.read_map.len() as i64 - 1);
        self.block_height - dist
    }

    pub fn get_read_rev(&self, acc_addr: Address) -> Option<&ReadRevAccessItem> {
        self.read_rev_map.get(&acc_addr)
    }

    pub fn get_write_rev(&self, acc_addr: Address) -> Option<&WriteRevAccessItem> {
        self.write_rev_map.get(&acc_addr)
    }

    pub fn alloc_new_block(&mut self) {
        self.block_height = self.block_height.next_height();
        self.read_map.push_back(ReadAccessItem::default());
        self.write_map.push_back(WriteAccessItem::default());
    }

    pub fn add_read(&mut self, reads: &TxReadSet) {
        let read_entry = self
            .read_map
            .back_mut()
            .expect("AccessMap: Failed to acess read_map.");

        for (&acc_addr, read) in reads.iter() {
            let read_rev_entry = self.read_rev_map.entry(acc_addr).or_default();
            let acc_access = read_entry.entry(acc_addr).or_default();

            if read.get_nonce() {
                read_rev_entry.add_nonce(self.block_height);
                acc_access.set_nonce(true);
            }

            if read.get_code() {
                read_rev_entry.add_code(self.block_height);
                acc_access.set_code(true);
            }

            for &key in read.get_values().iter() {
                read_rev_entry.add_value(key, self.block_height);
                acc_access.add_value(key);
            }
        }
    }

    pub fn add_write(&mut self, writes: &TxWriteData) {
        let write_entry = self
            .write_map
            .back_mut()
            .expect("AccessMap: Failed to access write_map.");

        for (&acc_addr, write) in writes.iter() {
            let write_rev_entry = self.write_rev_map.entry(acc_addr).or_default();
            let acc_access = write_entry.entry(acc_addr).or_default();

            if write.nonce.is_some() {
                write_rev_entry.add_nonce(self.block_height);
                acc_access.set_nonce(true);
            }

            if write.code.is_some() {
                write_rev_entry.add_code(self.block_height);
                acc_access.set_code(true);
            }

            if write.reset_values {
                write_rev_entry.add_reset_values(self.block_height);
                acc_access.set_reset_values(true);
            }

            for &key in write.values.keys() {
                write_rev_entry.add_value(key, self.block_height);
                acc_access.add_value(key);
            }
        }
    }

    #[must_use]
    pub fn remove_oldest_block(&mut self) -> PruningData {
        let mut pruning = PruningData::default();

        if self.read_map.len() <= self.max_blocks {
            return pruning;
        }

        let old_block_height = self.oldest_block_height();

        let read_entry = self
            .read_map
            .pop_front()
            .expect("AccessMap: Failed to access read_map.");
        for (&acc_addr, read) in read_entry.iter() {
            let mut read_rev_entry = match self.read_rev_map.entry(acc_addr) {
                im::hashmap::Entry::Occupied(o) => o,
                im::hashmap::Entry::Vacant(_) => unreachable!(),
            };

            let entry = read_rev_entry.get_mut();

            if read.get_nonce() {
                entry.remove_nonce(old_block_height);
            }

            if read.get_code() {
                entry.remove_code(old_block_height);
            }

            for &key in read.value_iter() {
                entry.remove_value(key, old_block_height);
            }

            if entry.is_empty() {
                read_rev_entry.remove();
            }
        }

        let write_entry = self
            .write_map
            .pop_front()
            .expect("AccessMap: Failed to access write_map.");
        for (&acc_addr, write) in write_entry.iter() {
            let mut write_rev_entry = match self.write_rev_map.entry(acc_addr) {
                im::hashmap::Entry::Occupied(o) => o,
                im::hashmap::Entry::Vacant(_) => unreachable!(),
            };

            let entry = write_rev_entry.get_mut();

            if write.get_nonce() && entry.remove_nonce(old_block_height) {
                pruning.add_nonce(acc_addr);
            }

            if write.get_code() && entry.remove_code(old_block_height) {
                pruning.add_code(acc_addr);
            }

            if write.get_reset_values() && entry.remove_reset_values(old_block_height) {
                pruning.add_reset_values(acc_addr);
            }

            for &key in write.value_iter() {
                if entry.remove_value(key, old_block_height) {
                    pruning.add_value(acc_addr, key);
                }
            }

            if entry.is_empty() {
                write_rev_entry.remove();
            }
        }

        pruning
    }
}

#[cfg(test)]
mod tests;