use crate::access_map::{AccessMap, RevAccessItem};
use serde::Deserialize;
use slimchain_common::{
    basic::BlockHeight,
    rw_set::{TxReadSet, TxWriteData},
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictCheck {
    OCC,
    SSI,
}

impl ConflictCheck {
    pub fn has_conflict(
        self,
        access_map: &AccessMap,
        tx_block_height: BlockHeight,
        reads: &TxReadSet,
        writes: &TxWriteData,
    ) -> bool {
        match self {
            Self::OCC => occ_conflict_check(access_map, tx_block_height, reads, writes),
            Self::SSI => ssi_conflict_check(access_map, tx_block_height, reads, writes),
        }
    }
}

fn occ_conflict_check(
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

fn ssi_conflict_check(
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

#[cfg(test)]
mod tests {
    use super::*;
    use slimchain_common::{create_tx_read_set, create_tx_write_set};

    #[test]
    fn test_deserialize() {
        use slimchain_utils::toml;

        #[derive(Deserialize)]
        struct Test {
            conflict_check: ConflictCheck,
        }

        let input = toml::toml! { conflict_check = "occ" };
        assert_eq!(
            ConflictCheck::OCC,
            input.try_into::<Test>().unwrap().conflict_check
        );

        let input = toml::toml! { conflict_check = "ssi" };
        assert_eq!(
            ConflictCheck::SSI,
            input.try_into::<Test>().unwrap().conflict_check
        );
    }

    #[test]
    fn test_conflict_check() {
        let mut map = AccessMap::new(2);
        map.alloc_new_block();
        map.add_read(&create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000010",
                ]
            },
        });
        map.add_write(&create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000001" => 1,
                }
            },
        });
        let _ = map.remove_oldest_block();
        map.alloc_new_block();
        map.add_read(&create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000010",
                ]
            },
        });
        map.add_write(&create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                }
            },
        });
        let _ = map.remove_oldest_block();
        map.alloc_new_block();

        let tx1_read = create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000010",
                ]
            },
        };
        let tx1_write = create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000010" => 1,
                }
            },
        };
        assert!(!ConflictCheck::OCC.has_conflict(&map, 1.into(), &tx1_read, &tx1_write));
        assert!(!ConflictCheck::SSI.has_conflict(&map, 1.into(), &tx1_read, &tx1_write));

        let tx2_read = create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ]
            },
        };
        let tx2_write = create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000011" => 1,
                }
            },
        };
        assert!(ConflictCheck::OCC.has_conflict(&map, 1.into(), &tx2_read, &tx2_write));
        assert!(!ConflictCheck::SSI.has_conflict(&map, 1.into(), &tx2_read, &tx2_write));

        let tx3_read = create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ]
            },
        };
        let tx3_write = create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000010" => 1,
                }
            },
        };
        assert!(ConflictCheck::OCC.has_conflict(&map, 1.into(), &tx3_read, &tx3_write));
        assert!(ConflictCheck::SSI.has_conflict(&map, 1.into(), &tx3_read, &tx3_write));
    }
}
