use super::*;
use rand::SeedableRng;
use slimchain_common::{
    basic::U256,
    ed25519::Keypair,
    tx::TxTrait,
    tx_req::{caller_address_from_pk, TxRequest},
};
use slimchain_tx_engine::{TxEngine, TxTask, TxTaskOutput};
use slimchain_tx_state::{MemTxState, TxProposal};
use slimchain_utils::{
    config::Config,
    contract::{contract_address, Contract, Token},
    metrics::init_metrics_subscriber,
};
use std::path::PathBuf;

#[test]
fn test() {
    let _ = tracing_subscriber::fmt::try_init();
    let _ = init_metrics_subscriber(std::io::stdout());

    let mut states = MemTxState::new();

    let contract_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("contracts/build/contracts/SimpleStorage.json");
    let contract = Contract::from_json_file(&contract_file).unwrap();

    let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
    let keypair = Keypair::generate(&mut rng);
    let caller_address = caller_address_from_pk(&keypair.public);
    let contract_address = contract_address(caller_address, U256::from(0).into());

    let cfg = Config::load_test().unwrap();
    let factory = TEETxEngineWorkerFactory::use_test(cfg.get("tee").unwrap()).unwrap();

    let task_engine = TxEngine::new(2, || factory.worker());

    let tx_req1 = TxRequest::Create {
        nonce: U256::from(0).into(),
        code: contract.code().clone(),
    };
    let signed_tx_req1 = tx_req1.sign(&keypair);

    let task1 = TxTask::new(
        1.into(),
        states.state_view(),
        states.state_root(),
        signed_tx_req1,
    );
    task_engine.push_task(task1);
    let TxTaskOutput {
        tx_proposal:
            TxProposal {
                tx: tx1,
                write_trie: write_trie1,
            },
        ..
    } = task_engine.pop_or_wait_result();
    assert!(write_trie1.verify(states.state_root()).is_ok());
    assert!(tx1.verify_sig().is_ok());

    assert!(tx1
        .raw_tx
        .writes
        .get(&contract_address)
        .unwrap()
        .values
        .iter()
        .any(|(_k, v)| v.to_low_u64_be() == 42));
    states.apply_writes(&tx1.raw_tx.writes).unwrap();

    let tx_req2 = TxRequest::Call {
        address: contract_address,
        nonce: U256::from(1).into(),
        data: contract
            .encode_tx_input(
                "set",
                &[Token::Uint(U256::from(1)), Token::Uint(U256::from(43))],
            )
            .unwrap(),
    };
    let signed_tx_req2 = tx_req2.sign(&keypair);

    let task2 = TxTask::new(
        2.into(),
        states.state_view(),
        states.state_root(),
        signed_tx_req2,
    );
    task_engine.push_task(task2);
    let TxTaskOutput {
        tx_proposal:
            TxProposal {
                tx: tx2,
                write_trie: write_trie2,
            },
        ..
    } = task_engine.pop_or_wait_result();
    assert!(write_trie2.verify(states.state_root()).is_ok());
    assert!(tx2.verify_sig().is_ok());

    assert!(tx2
        .raw_tx
        .writes
        .get(&contract_address)
        .unwrap()
        .values
        .iter()
        .any(|(_k, v)| v.to_low_u64_be() == 43));
}
