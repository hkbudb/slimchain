use crate::{
    behavior::{propose_block, verify_block},
    config::{ChainConfig, MinerConfig},
    conflict_check::ConflictCheck,
    consensus::{
        raft::{create_new_block, verify_consensus, Block},
        Consensus,
    },
};
use slimchain_common::{
    ed25519::Keypair,
    error::Result,
    tx::{SignedTx, TxTrait},
};
use slimchain_utils::init_tracing_for_test;
use std::time::Duration;

#[tokio::test]
async fn test_chain_cycle() {
    let _gurad = init_tracing_for_test();

    let chain_cfg = ChainConfig {
        conflict_check: ConflictCheck::SSI,
        state_len: 2,
        consensus: Consensus::Raft,
    };
    let miner_cfg = MinerConfig {
        max_txs: 1,
        min_txs: 1,
        max_block_interval: Duration::from_millis(100),
    };
}
