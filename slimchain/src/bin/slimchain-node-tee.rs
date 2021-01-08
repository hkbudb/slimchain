use slimchain_common::error::Result;
use slimchain_tx_engine::TxEngine;
use slimchain_utils::config::Config;
use std::path::PathBuf;

use slimchain_tee_sig::TEESignedTx as Tx;

#[cfg(target_os = "linux")]
fn create_tx_engine(cfg: &Config, enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
    use slimchain_tx_engine_tee::{TEEConfig, TEETxEngineWorkerFactory};
    use slimchain_utils::tx_engine_threads;

    let tee_cfg: TEEConfig = cfg.get("tee")?;
    let factory = match enclave {
        Some(enclave) => TEETxEngineWorkerFactory::new(tee_cfg, enclave)?,
        None => TEETxEngineWorkerFactory::use_enclave_in_the_same_dir(tee_cfg)?,
    };
    Ok(TxEngine::new(tx_engine_threads(), || factory.worker()))
}

#[cfg(not(target_os = "linux"))]
fn create_tx_engine(_cfg: &Config, _enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
    use slimchain_common::error::bail;

    bail!("not support!");
}

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::node::node_main(create_tx_engine).await
}
