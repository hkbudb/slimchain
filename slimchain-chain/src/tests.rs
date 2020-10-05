use crate::{
    behavior::{
        commit_block, commit_block_storage_node, propose_block, verify_block, TxExecuteStream,
    },
    block_proposal::BlockProposal,
    config::{ChainConfig, MinerConfig},
    conflict_check::ConflictCheck,
    consensus::{
        raft::{create_new_block, verify_consensus, Block},
        Consensus,
    },
    db::DB,
    latest::LatestTxCount,
    snapshot::Snapshot,
};
use futures::{channel::mpsc::unbounded, prelude::*};
use rand::SeedableRng;
use slimchain_common::{
    basic::{ShardId, U256},
    ed25519::Keypair,
    tx::SignedTx,
    tx_req::{caller_address_from_pk, TxRequest},
};
use slimchain_tx_engine::TxEngine;
use slimchain_tx_engine_simple::SimpleTxEngineWorker;
use slimchain_tx_state::{StorageTxTrie, TxTrie};
use slimchain_utils::{
    contract::{contract_address, Contract, Token},
    init_tracing_for_test,
};
use std::{path::PathBuf, time::Duration};

async fn test_chain_cycle(chain_cfg: &ChainConfig, miner_cfg: &MinerConfig) {
    let contract_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("contracts/build/contracts/SimpleStorage.json");
    let contract = Contract::from_json_file(&contract_file).unwrap();

    let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
    let keypair = Keypair::generate(&mut rng);
    let caller_address = caller_address_from_pk(&keypair.public);
    let contract_address = contract_address(caller_address, U256::from(0).into());

    let task_engine = TxEngine::new(2, || {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
        Box::new(SimpleTxEngineWorker::new(Keypair::generate(&mut rng)))
    });

    let client_db = DB::load_test();
    let storage_db = DB::load_test();
    let miner_db = DB::load_test();

    let mut client_snapshot =
        Snapshot::<Block, TxTrie>::load_from_db(&client_db, chain_cfg.state_len).unwrap();
    let mut miner_snapshot =
        Snapshot::<Block, TxTrie>::load_from_db(&miner_db, chain_cfg.state_len).unwrap();
    let mut storage_snapshot = Snapshot::<Block, StorageTxTrie>::load_from_db(
        &storage_db,
        chain_cfg.state_len,
        ShardId::default(),
    )
    .unwrap();

    let client_blk_latest = client_snapshot.to_latest_block_header();
    let miner_blk_latest = miner_snapshot.to_latest_block_header();
    let storage_blk_latest = storage_snapshot.to_latest_block_header();

    let client_tx_latest = LatestTxCount::new(0);
    let miner_tx_latest = LatestTxCount::new(0);
    let storage_tx_latest = LatestTxCount::new(0);

    let (mut req_tx, req_rx) = unbounded();
    let mut tx_rx = TxExecuteStream::new(req_rx, task_engine, &storage_db, &storage_blk_latest);

    let mut tx_reqs = vec![TxRequest::Create {
        nonce: U256::from(0).into(),
        code: contract.code().clone(),
    }];

    for i in 0..5 {
        tx_reqs.push(TxRequest::Call {
            address: contract_address,
            nonce: U256::from(i + 1).into(),
            data: contract
                .encode_tx_input(
                    "set",
                    &[Token::Uint(U256::from(i)), Token::Uint(U256::from(i))],
                )
                .unwrap(),
        });
    }

    for tx_req in tx_reqs {
        let signed_tx_req = tx_req.sign(&keypair);
        req_tx.send(signed_tx_req).await.unwrap();
        let blk_proposal = propose_block(
            chain_cfg,
            miner_cfg,
            &mut miner_snapshot,
            &mut tx_rx,
            create_new_block,
        )
        .await
        .unwrap()
        .unwrap();
        verify_block(
            chain_cfg,
            &mut client_snapshot,
            &blk_proposal,
            verify_consensus,
        )
        .await
        .unwrap();
        let storage_update = verify_block(
            chain_cfg,
            &mut storage_snapshot,
            &blk_proposal,
            verify_consensus,
        )
        .await
        .unwrap();

        commit_block(
            &blk_proposal,
            &miner_db,
            &miner_blk_latest,
            &miner_tx_latest,
        )
        .await
        .unwrap();
        commit_block(
            &blk_proposal,
            &client_db,
            &client_blk_latest,
            &client_tx_latest,
        )
        .await
        .unwrap();
        commit_block_storage_node(
            &blk_proposal,
            &storage_update,
            &storage_db,
            &storage_blk_latest,
            &storage_tx_latest,
        )
        .await
        .unwrap();
    }

    let client2_db = DB::load_test();
    let mut client2_snapshot =
        Snapshot::<Block, TxTrie>::load_from_db(&client2_db, chain_cfg.state_len).unwrap();
    let client2_blk_latest = client2_snapshot.to_latest_block_header();
    let client2_tx_latest = LatestTxCount::new(0);
    for i in 1..=6 {
        let blk_proposal: BlockProposal<Block, SignedTx> =
            BlockProposal::from_db(&storage_db, i.into()).unwrap();
        verify_block(
            chain_cfg,
            &mut client2_snapshot,
            &blk_proposal,
            verify_consensus,
        )
        .await
        .unwrap();
        commit_block(
            &blk_proposal,
            &client_db,
            &client2_blk_latest,
            &client2_tx_latest,
        )
        .await
        .unwrap();
    }

    let latest_blk = miner_snapshot.get_latest_block();

    assert_eq!(latest_blk, client_snapshot.get_latest_block());
    assert_eq!(latest_blk, client2_snapshot.get_latest_block());
    assert_eq!(latest_blk, storage_snapshot.get_latest_block());

    client_snapshot.write_async(&client_db).await.unwrap();
    miner_snapshot.write_async(&miner_db).await.unwrap();
    storage_snapshot.write_async(&storage_db).await.unwrap();

    let client_snapshot2 =
        Snapshot::<Block, TxTrie>::load_from_db(&client_db, chain_cfg.state_len).unwrap();
    let miner_snapshot2 =
        Snapshot::<Block, TxTrie>::load_from_db(&miner_db, chain_cfg.state_len).unwrap();
    let storage_snapshot2 = Snapshot::<Block, StorageTxTrie>::load_from_db(
        &storage_db,
        chain_cfg.state_len,
        ShardId::default(),
    )
    .unwrap();

    assert_eq!(
        client_snapshot.recent_blocks,
        client_snapshot2.recent_blocks
    );
    assert_eq!(client_snapshot.access_map, client_snapshot2.access_map);
    assert_eq!(miner_snapshot.recent_blocks, miner_snapshot2.recent_blocks);
    assert_eq!(miner_snapshot.access_map, miner_snapshot2.access_map);
    assert_eq!(
        storage_snapshot.recent_blocks,
        storage_snapshot2.recent_blocks
    );
    assert_eq!(storage_snapshot.access_map, storage_snapshot2.access_map);

    assert_eq!(client_tx_latest.get(), client2_tx_latest.get());
    assert_eq!(client_tx_latest.get(), miner_tx_latest.get());
    assert_eq!(client_tx_latest.get(), storage_tx_latest.get());
}

#[tokio::test]
async fn test() {
    let _guard = init_tracing_for_test();

    let miner_cfg = MinerConfig {
        max_txs: 1,
        min_txs: 1,
        max_block_interval: Duration::from_millis(100),
    };

    for state_len in 1..=3 {
        for &conflict_check in &[ConflictCheck::SSI, ConflictCheck::OCC] {
            let chain_cfg = ChainConfig {
                conflict_check,
                state_len,
                consensus: Consensus::Raft,
            };
            warn!(state_len, ?conflict_check);
            test_chain_cycle(&chain_cfg, &miner_cfg).await;
        }
    }
}
