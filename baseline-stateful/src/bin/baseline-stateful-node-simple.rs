use slimchain_common::error::Result;
use slimchain_tx_engine::TxEngine;
use slimchain_utils::{config::Config, tx_engine_threads};
use std::path::PathBuf;

use slimchain_common::tx::SignedTx as Tx;
use slimchain_tx_engine_simple::SimpleTxEngineWorker;

fn create_tx_engine(_cfg: &Config, _enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
    Ok(TxEngine::new(tx_engine_threads(), || {
        let mut rng = rand::thread_rng();
        let keypair = slimchain_common::ed25519::Keypair::generate(&mut rng);
        Box::new(SimpleTxEngineWorker::new(keypair))
    }))
}

fn main() -> Result<()> {
    use tokio::runtime::Builder;
    Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024) // increase thread stack size
        .build()
        .unwrap()
        .block_on(async { baseline_stateful::node::node_main(create_tx_engine).await })
}
