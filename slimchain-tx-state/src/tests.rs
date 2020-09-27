use super::*;
use core::iter;
use slimchain_common::{
    basic::ShardId, create_address, create_state_key, create_tx_read_data, create_tx_write_set,
};
use slimchain_merkle_trie::prelude::*;

#[cfg(all(feature = "read", feature = "write"))]
#[test]
fn test_read_write() {
    let write1 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 1,
        },
        "0000000000000000000000000000000000000010" => {
            nonce: 2,
            reset_values: true,
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 1,
                "0000000000000000000000000000000000000000000000000000000000000002" => 2,
                "0000000000000000000000000000000000000000000000000000000000001001" => 3,
                "0000000000000000000000000000000000000000000000000000000001001002" => 4,
            }
        },
    };

    let mut state = MemTxState::new();
    let update1 = update_tx_state(&state.state_view(), state.state_root(), &write1).unwrap();
    state.apply_update(update1).unwrap();

    let read1 = create_tx_read_data! {
        "0000000000000000000000000000000000000000" => {
            nonce: 1,
            code: b"",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 0,
            }
        },
        "0000000000000000000000000000000000000010" => {
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 1,
                "0000000000000000000000000000000000000000000000000000000000001001" => 3,
            }
        },
        "0000000000000000000000000000000000000100" => {
            nonce: 0,
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 0,
            }
        }
    };

    let mut read_ctx1 = TxStateReadContext::new(state.state_view(), state.state_root());
    let acc_addr1 = create_address!("0000000000000000000000000000000000000000");
    let acc_addr2 = create_address!("0000000000000000000000000000000000000010");
    let acc_addr3 = create_address!("0000000000000000000000000000000000000100");
    assert_eq!(read_ctx1.get_nonce(acc_addr1).unwrap(), 1.into());
    assert_eq!(read_ctx1.get_code_len(acc_addr1).unwrap(), 0);
    assert_eq!(
        read_ctx1
            .get_value(
                acc_addr1,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                )
            )
            .unwrap(),
        0.into()
    );
    assert_eq!(
        read_ctx1.get_code(acc_addr2).unwrap(),
        b"code".to_vec().into()
    );
    assert_eq!(
        read_ctx1
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                )
            )
            .unwrap(),
        1.into()
    );
    assert_eq!(
        read_ctx1
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000001001"
                )
            )
            .unwrap(),
        3.into()
    );
    assert_eq!(read_ctx1.get_nonce(acc_addr3).unwrap(), 0.into());
    assert_eq!(
        read_ctx1
            .get_value(
                acc_addr3,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                )
            )
            .unwrap(),
        0.into()
    );
    let read_proof1 = read_ctx1.generate_proof().unwrap();
    assert!(read_proof1.verify(&read1, state.state_root()).is_ok());

    let write2 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000010" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000002" => 7,
                "1000000000000000000000000000000000000000000000000000000000000000" => 8,
            }
        },
    };
    let update2 = update_tx_state(&state.state_view(), state.state_root(), &write2).unwrap();
    state.apply_update(update2).unwrap();

    let read2 = create_tx_read_data! {
        "0000000000000000000000000000000000000000" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000010" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000002" => 7,
                "1000000000000000000000000000000000000000000000000000000000000000" => 8,
            }
        },
    };
    let mut read_ctx2 = TxStateReadContext::new(state.state_view(), state.state_root());
    assert_eq!(read_ctx2.get_nonce(acc_addr1).unwrap(), 2.into());
    assert_eq!(
        read_ctx2
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000002"
                )
            )
            .unwrap(),
        7.into()
    );
    assert_eq!(
        read_ctx2
            .get_value(
                acc_addr2,
                create_state_key!(
                    "1000000000000000000000000000000000000000000000000000000000000000"
                )
            )
            .unwrap(),
        8.into()
    );
    let read_proof2 = read_ctx2.generate_proof().unwrap();
    assert!(read_proof2.verify(&read2, state.state_root()).is_ok());

    let read3 = create_tx_read_data! {
        "0000000000000000000000000000000000000010" => {
            nonce: 2,
        },
    };
    let mut read_ctx3 = TxStateReadContext::new(state.state_view(), state.state_root());
    assert_eq!(read_ctx3.get_nonce(acc_addr2).unwrap(), 2.into());
    let read_proof3 = read_ctx3.generate_proof().unwrap();
    assert!(read_proof3.verify(&read3, state.state_root()).is_ok());
}

#[cfg(feature = "partial_trie")]
#[test]
fn test_tx_trie() {
    let write_set1 = create_tx_write_set! {
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
        },
    };
    let write_set2 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000001" => {
            nonce: 1,
        },
        "0000000000000000000000000000000000000002" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                "0000000000000000000000000000000000000000000000000000000000000001" => 2,
            }
        },
        "0000000000000000000000000000000000001001" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000001000" => 3,
                "0000000000000000000000000000000000000000000000000000000000001001" => 4,
            }
        },
    };
    let write_set3 = create_tx_write_set! {
        "0000000000000000000000000000000000000001" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000002" => {
            nonce: 1,
        },
        "0000000000000000000000000000100000000004" => {
            reset_values: true,
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                "0000000000000000000000000000000000000000000000000000000000000001" => 2,
            }
        },
        "0000000000000000000000000000000000001001" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000002" => 3,
                "0000000000000000000000000000000000000000000000000000000000000003" => 4,
            }
        },
    };
    let write_set4 = create_tx_write_set! {
        "0000000000000000000000000000000000000100" => {
            nonce: 1,
        },
        "0000000000000000000000000000000000001001" => {
            values: {
                "1000000000000000000000000000000000000000000000000000000000000000" => 1,
                "0000000000000000000000000000000000000000000000000000000000001001" => 2,
                "0000000000000000000000000000000000000000000000000000000000000005" => 3,
            }
        },
    };

    let mut full_node_storage = MemTxState::new();
    let mut full_node = StorageTxTrie::new(
        ShardId::new(0, 1),
        InShardData::new(
            full_node_storage.state_view(),
            full_node_storage.state_root(),
        ),
        OutShardData::default(),
    );

    let mut shard_node1_storage = MemTxState::new();
    let mut shard_node1 = StorageTxTrie::new(
        ShardId::new(0, 2),
        InShardData::new(
            shard_node1_storage.state_view(),
            shard_node1_storage.state_root(),
        ),
        OutShardData::default(),
    );

    let mut shard_node2_storage = MemTxState::new();
    let mut shard_node2 = StorageTxTrie::new(
        ShardId::new(1, 2),
        InShardData::new(
            shard_node2_storage.state_view(),
            shard_node2_storage.state_root(),
        ),
        OutShardData::default(),
    );

    let mut shard_node3_storage = MemTxState::new();
    let mut shard_node3 = StorageTxTrie::new(
        ShardId::new(0, 2),
        InShardData::new(
            shard_node3_storage.state_view(),
            shard_node3_storage.state_root(),
        ),
        OutShardData::default(),
    );

    let mut shard_node4_storage = MemTxState::new();
    let mut shard_node4 = StorageTxTrie::new(
        ShardId::new(1, 2),
        InShardData::new(
            shard_node4_storage.state_view(),
            shard_node4_storage.state_root(),
        ),
        OutShardData::default(),
    );

    let mut client1 = TxTrie::default();
    let mut client2 = TxTrie::default();
    let mut client3 = TxTrie::default();

    let write_set1_trie = TxWriteSetTrie::new(
        &full_node_storage.state_view(),
        full_node_storage.state_root(),
        &write_set1,
    )
    .unwrap();
    assert!(write_set1_trie.verify(client1.root_hash()).is_ok());
    let write_set1_diff = client1.diff_missing_branches(&write_set1_trie);

    full_node.apply_diff(&write_set1_diff, true).unwrap();
    let update = full_node.apply_writes(&write_set1).unwrap();
    full_node_storage.apply_update(update).unwrap();

    shard_node1.apply_diff(&write_set1_diff, true).unwrap();
    let update = shard_node1.apply_writes(&write_set1).unwrap();
    shard_node1_storage.apply_update(update).unwrap();
    shard_node1.out_shard.0.clear();

    shard_node2.apply_diff(&write_set1_diff, true).unwrap();
    let update = shard_node2.apply_writes(&write_set1).unwrap();
    shard_node2_storage.apply_update(update).unwrap();
    shard_node2.out_shard.0.clear();

    shard_node3
        .update_missing_branches(&write_set1_trie)
        .unwrap();
    let update = shard_node3.apply_writes(&write_set1).unwrap();
    shard_node3_storage.apply_update(update).unwrap();
    shard_node3.out_shard.0.clear();

    shard_node4
        .update_missing_branches(&write_set1_trie)
        .unwrap();
    let update = shard_node4.apply_writes(&write_set1).unwrap();
    shard_node4_storage.apply_update(update).unwrap();
    shard_node4.out_shard.0.clear();

    client1.apply_diff(&write_set1_diff, true).unwrap();
    client1.apply_writes(&write_set1).unwrap();
    client1.main_trie = PartialTrie::from_root_hash(client1.root_hash());
    client1.acc_tries.clear();

    client2.apply_diff(&write_set1_diff, true).unwrap();
    client2.apply_writes(&write_set1).unwrap();
    client2.main_trie = PartialTrie::from_root_hash(client2.root_hash());
    client2.acc_tries.clear();

    client3.update_missing_branches(&write_set1_trie).unwrap();
    client3.apply_writes(&write_set1).unwrap();
    client3.main_trie = PartialTrie::from_root_hash(client3.root_hash());
    client3.acc_tries.clear();

    assert_eq!(
        full_node_storage.state_root(),
        shard_node1_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node2_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node3_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node4_storage.state_root()
    );
    assert_eq!(full_node_storage.state_root(), client1.root_hash());
    assert_eq!(full_node_storage.state_root(), client2.root_hash());
    assert_eq!(full_node_storage.state_root(), client3.root_hash());

    let write_set2_trie = TxWriteSetTrie::new(
        &full_node_storage.state_view(),
        full_node_storage.state_root(),
        &write_set2,
    )
    .unwrap();
    assert!(write_set2_trie.verify(client1.root_hash()).is_ok());
    let write_set2_diff = client1.diff_missing_branches(&write_set2_trie);

    client1.apply_diff(&write_set2_diff, true).unwrap();
    client1.apply_writes(&write_set2).unwrap();

    client2.apply_diff(&write_set2_diff, true).unwrap();
    client2.apply_writes(&write_set2).unwrap();

    client3.update_missing_branches(&write_set2_trie).unwrap();
    client3.apply_writes(&write_set2).unwrap();

    full_node.apply_diff(&write_set2_diff, true).unwrap();
    let update = full_node.apply_writes(&write_set2).unwrap();
    full_node_storage.apply_update(update).unwrap();

    shard_node1.apply_diff(&write_set2_diff, true).unwrap();
    let update = shard_node1.apply_writes(&write_set2).unwrap();
    shard_node1_storage.apply_update(update).unwrap();

    shard_node2.apply_diff(&write_set2_diff, true).unwrap();
    let update = shard_node2.apply_writes(&write_set2).unwrap();
    shard_node2_storage.apply_update(update).unwrap();

    shard_node3
        .update_missing_branches(&write_set2_trie)
        .unwrap();
    let update = shard_node3.apply_writes(&write_set2).unwrap();
    shard_node3_storage.apply_update(update).unwrap();

    shard_node4
        .update_missing_branches(&write_set2_trie)
        .unwrap();
    let update = shard_node4.apply_writes(&write_set2).unwrap();
    shard_node4_storage.apply_update(update).unwrap();

    assert_eq!(
        full_node_storage.state_root(),
        shard_node1_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node2_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node3_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node4_storage.state_root()
    );
    assert_eq!(full_node_storage.state_root(), client1.root_hash());
    assert_eq!(full_node_storage.state_root(), client2.root_hash());
    assert_eq!(full_node_storage.state_root(), client3.root_hash());

    let write_set3_trie = TxWriteSetTrie::new(
        &full_node_storage.state_view(),
        full_node_storage.state_root(),
        &write_set3,
    )
    .unwrap();
    assert!(write_set3_trie.verify(client1.root_hash()).is_ok());
    let write_set3_diff = client1.diff_missing_branches(&write_set3_trie);

    let write_set4_trie = TxWriteSetTrie::new(
        &full_node_storage.state_view(),
        full_node_storage.state_root(),
        &write_set4,
    )
    .unwrap();
    assert!(write_set4_trie.verify(client1.root_hash()).is_ok());
    let write_set4_diff = client1.diff_missing_branches(&write_set4_trie);

    let write_set34_diff = merge_tx_trie_diff(&write_set3_diff, &write_set4_diff);

    client1.apply_diff(&write_set34_diff, true).unwrap();
    client1.apply_writes(&write_set3).unwrap();
    client1.apply_writes(&write_set4).unwrap();

    client2.apply_diff(&write_set34_diff, true).unwrap();
    client2.apply_writes(&write_set3).unwrap();
    client2.apply_writes(&write_set4).unwrap();

    client3.update_missing_branches(&write_set3_trie).unwrap();
    client3.update_missing_branches(&write_set4_trie).unwrap();
    client3.apply_writes(&write_set3).unwrap();
    client3.apply_writes(&write_set4).unwrap();

    full_node.apply_diff(&write_set34_diff, true).unwrap();
    let update = full_node.apply_writes(&write_set3).unwrap();
    full_node_storage.apply_update(update).unwrap();
    let update = full_node.apply_writes(&write_set4).unwrap();
    full_node_storage.apply_update(update).unwrap();

    shard_node1.apply_diff(&write_set34_diff, true).unwrap();
    let update = shard_node1.apply_writes(&write_set3).unwrap();
    shard_node1_storage.apply_update(update).unwrap();
    let update = shard_node1.apply_writes(&write_set4).unwrap();
    shard_node1_storage.apply_update(update).unwrap();

    shard_node2.apply_diff(&write_set34_diff, true).unwrap();
    let update = shard_node2.apply_writes(&write_set3).unwrap();
    shard_node2_storage.apply_update(update).unwrap();
    let update = shard_node2.apply_writes(&write_set4).unwrap();
    shard_node2_storage.apply_update(update).unwrap();

    shard_node3
        .update_missing_branches(&write_set3_trie)
        .unwrap();
    shard_node3
        .update_missing_branches(&write_set4_trie)
        .unwrap();
    let update = shard_node3.apply_writes(&write_set3).unwrap();
    shard_node3_storage.apply_update(update).unwrap();
    let update = shard_node3.apply_writes(&write_set4).unwrap();
    shard_node3_storage.apply_update(update).unwrap();

    shard_node4
        .update_missing_branches(&write_set3_trie)
        .unwrap();
    shard_node4
        .update_missing_branches(&write_set4_trie)
        .unwrap();
    let update = shard_node4.apply_writes(&write_set3).unwrap();
    shard_node4_storage.apply_update(update).unwrap();
    let update = shard_node4.apply_writes(&write_set4).unwrap();
    shard_node4_storage.apply_update(update).unwrap();

    assert_eq!(
        full_node_storage.state_root(),
        shard_node1_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node2_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node3_storage.state_root()
    );
    assert_eq!(
        full_node_storage.state_root(),
        shard_node4_storage.state_root()
    );
    assert_eq!(full_node_storage.state_root(), client1.root_hash());
    assert_eq!(full_node_storage.state_root(), client2.root_hash());
    assert_eq!(full_node_storage.state_root(), client3.root_hash());
}

#[cfg(feature = "partial_trie")]
#[test]
fn test_prune() {
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

    let addr1 = create_address!("0000000000000000000000000000000000000000");
    let addr2 = create_address!("0000000000000000000000000000000000000001");

    let key1 =
        create_state_key!("0000000000000000000000000000000000000000000000000000000000000000");
    let key2 =
        create_state_key!("0000000000000000000000000000000000000000000000000000000000000001");

    let mut trie1 = TxTrie::default();
    trie1.apply_writes(&write_set).unwrap();
    let root = trie1.root_hash();

    trie1.prune_account(addr1, iter::once(addr2)).unwrap();
    assert_eq!(trie1.acc_tries.len(), 1);
    assert_eq!(trie1.root_hash(), root);

    trie1
        .prune_acc_state_key(addr2, key1, iter::once(key2))
        .unwrap();
    assert_eq!(trie1.acc_tries.len(), 1);
    assert_eq!(trie1.root_hash(), root);

    trie1.prune_account(addr2, iter::empty()).unwrap();
    assert_eq!(trie1.acc_tries.len(), 0);
    assert_eq!(trie1.root_hash(), root);

    let mut trie2_storage = MemTxState::new();
    let mut trie2 = StorageTxTrie::new(
        ShardId::new(0, 2),
        InShardData::new(trie2_storage.state_view(), trie2_storage.state_root()),
        OutShardData::default(),
    );
    let update = trie2.apply_writes(&write_set).unwrap();
    trie2_storage.apply_update(update).unwrap();
    assert_eq!(trie2.out_shard.len(), 1);

    trie2.prune_account(addr1, iter::once(addr2)).unwrap();
    assert_eq!(trie2.out_shard.len(), 1);
    assert_eq!(trie2.root_hash(), root);

    trie2
        .prune_acc_state_key(addr2, key1, iter::once(key2))
        .unwrap();
    assert_eq!(trie2.out_shard.len(), 1);
    assert_eq!(trie2.root_hash(), root);

    trie2.prune_account(addr2, iter::empty()).unwrap();
    assert_eq!(trie2.out_shard.len(), 0);
    assert_eq!(trie2.root_hash(), root);

    let mut trie3_storage = MemTxState::new();
    let mut trie3 = StorageTxTrie::new(
        ShardId::new(1, 2),
        InShardData::new(trie3_storage.state_view(), trie3_storage.state_root()),
        OutShardData::default(),
    );
    let update = trie3.apply_writes(&write_set).unwrap();
    trie3_storage.apply_update(update).unwrap();
    assert_eq!(trie3.out_shard.len(), 0);

    trie3.prune_account(addr1, iter::once(addr2)).unwrap();
    assert_eq!(trie3.out_shard.len(), 0);
    assert_eq!(trie3.root_hash(), root);

    trie3
        .prune_acc_state_key(addr2, key1, iter::once(key2))
        .unwrap();
    assert_eq!(trie3.out_shard.len(), 0);
    assert_eq!(trie3.root_hash(), root);

    trie3.prune_account(addr2, iter::empty()).unwrap();
    assert_eq!(trie3.out_shard.len(), 0);
    assert_eq!(trie3.root_hash(), root);
}
