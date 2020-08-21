use crate::access_map::{AccessMap, RevAccessItem};
use slimchain_common::{
    basic::BlockHeight,
    rw_set::{TxReadSet, TxWriteData},
};

pub trait ConflictCheck {
    fn has_conflict(
        access_map: &AccessMap,
        tx_block_height: BlockHeight,
        reads: &TxReadSet,
        writes: &TxWriteData,
    ) -> bool;
}

pub struct OCCConflictCheck;

impl ConflictCheck for OCCConflictCheck {
    fn has_conflict(
        access_map: &AccessMap,
        tx_block_height: BlockHeight,
        reads: &TxReadSet,
        writes: &TxWriteData,
    ) -> bool {
        for (&acc_addr, acc_read) in reads.iter() {
            let write_rev_map = match access_map.get_write_rev(acc_addr) {
                Some(entry) => entry,
                None => continue,
            };

            if write_rev_map.has_conflict_in_read_set(tx_block_height, acc_read) {
                return true;
            }
        }

        for (&acc_addr, acc_write) in writes.iter() {
            let write_rev_map = match access_map.get_write_rev(acc_addr) {
                Some(entry) => entry,
                None => continue,
            };

            if write_rev_map.has_conflict_in_write_set(tx_block_height, acc_write) {
                return true;
            }
        }

        false
    }
}

pub struct SSIConflictCheck;

impl ConflictCheck for SSIConflictCheck {
    fn has_conflict(
        access_map: &AccessMap,
        tx_block_height: BlockHeight,
        reads: &TxReadSet,
        writes: &TxWriteData,
    ) -> bool {
        let mut flag1 = false;
        let mut flag2 = false;

        for (&acc_addr, acc_write) in writes.iter() {
            let write_rev_map = match access_map.get_write_rev(acc_addr) {
                Some(entry) => entry,
                None => continue,
            };

            if write_rev_map.has_conflict_in_write_set(tx_block_height, acc_write) {
                return true;
            }

            let read_rev_map = match access_map.get_read_rev(acc_addr) {
                Some(entry) => entry,
                None => continue,
            };

            flag1 |= read_rev_map.has_conflict_in_write_set(tx_block_height, acc_write);
        }

        for (&acc_addr, acc_read) in reads.iter() {
            let write_rev_map = match access_map.get_write_rev(acc_addr) {
                Some(entry) => entry,
                None => continue,
            };

            flag2 |= write_rev_map.has_conflict_in_read_set(tx_block_height, acc_read);
        }

        flag1 && flag2
    }
}
