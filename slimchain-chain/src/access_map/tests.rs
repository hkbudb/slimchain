use super::*;
use slimchain_common::{create_tx_read_set, create_tx_write_set};

#[test]
fn test_access_map1() {
    let write_set = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 1,
        },
        "0000000000000000000000000000000000000001" => {
            reset_values: true,
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                "0000000000000000000000000000000000000000000000000000000000000001" => 2,
            }
        }
    };
    let read_set = create_tx_read_set! {
        "0000000000000000000000000000000000000002" => {
            nonce: true,
        },
        "0000000000000000000000000000000000000003" => {
            code: true,
            values: [
                "0000000000000000000000000000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000001",
            ]
        }
    };
    let mut map = AccessMap::new(1);
    map.alloc_new_block();
    map.add_read(&read_set);
    map.add_write(&write_set);
    assert_eq!(map.read_map.len(), 2);
    assert_eq!(map.write_map.len(), 2);
    assert_eq!(map.read_rev_map.len(), 2);
    assert_eq!(map.write_rev_map.len(), 2);
    let prune = map.remove_oldest_block();
    assert_eq!(prune.accounts.len(), 0);
    assert_eq!(prune.values.len(), 0);
    map.alloc_new_block();
    let prune = map.remove_oldest_block();
    assert_eq!(map.read_map.len(), 1);
    assert_eq!(map.write_map.len(), 1);
    assert_eq!(map.read_rev_map.len(), 0);
    assert_eq!(map.write_rev_map.len(), 0);
    assert_eq!(prune.accounts.len(), 2);
    assert_eq!(prune.values.len(), 0);
}

#[test]
fn test_access_map2() {
    let write_set1 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                "0000000000000000000000000000000000000000000000000000000000000001" => 2,
            }
        },
    };
    let write_set2 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            reset_values: true,
        },
    };
    let mut map = AccessMap::new(2);
    map.alloc_new_block();
    map.add_write(&write_set1);
    let _ = map.remove_oldest_block();
    map.alloc_new_block();
    map.add_write(&write_set2);
    let _ = map.remove_oldest_block();
    map.alloc_new_block();
    let prune = map.remove_oldest_block();
    assert_eq!(prune.accounts.len(), 0);
    assert_eq!(prune.values.len(), 0);
    map.alloc_new_block();
    let prune = map.remove_oldest_block();
    assert_eq!(prune.accounts.len(), 1);
    assert_eq!(prune.values.len(), 0);
}

#[test]
fn test_access_map3() {
    let write_set1 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000000" => 1,
            }
        },
    };
    let write_set2 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 1,
            }
        },
    };
    let mut map = AccessMap::new(2);
    map.alloc_new_block();
    map.add_write(&write_set1);
    let _ = map.remove_oldest_block();
    map.alloc_new_block();
    map.add_write(&write_set2);
    let _ = map.remove_oldest_block();
    map.alloc_new_block();
    let prune = map.remove_oldest_block();
    assert_eq!(prune.accounts.len(), 0);
    assert_eq!(prune.values.len(), 1);
    map.alloc_new_block();
    let prune = map.remove_oldest_block();
    assert_eq!(prune.accounts.len(), 1);
    assert_eq!(prune.values.len(), 0);
}
