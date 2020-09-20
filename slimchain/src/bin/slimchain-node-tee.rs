use slimchain_common::error::Result;
use slimchain_tx_engine::TxEngine;
use slimchain_utils::{config::Config, tx_engine_threads};
use std::path::PathBuf;

use slimchain_tee_sig::TEESignedTx as Tx;
use slimchain_tx_engine_tee::{TEEConfig, TEETxEngineWorkerFactory};

fn create_tx_engine(cfg: &Config, enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
    let tee_cfg: TEEConfig = cfg.get("tee")?;
    let factory = match enclave {
        Some(enclave) => TEETxEngineWorkerFactory::new(tee_cfg, enclave)?,
        None => TEETxEngineWorkerFactory::use_enclave_in_the_same_dir(tee_cfg)?,
    };
    Ok(TxEngine::new(tx_engine_threads(), || factory.worker()))
}

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::node::node_main(create_tx_engine).await
}
