#[macro_use]
extern crate tracing;
use slimchain_chain::{
    config::{ChainConfig, MinerConfig},
    consensus::Consensus,
    db::DB,
    role::Role,
};
use slimchain_common::error::Result;
use slimchain_network::{
    config::{NetworkConfig, PeerConfig},
    control::Swarmer,
};
use slimchain_tx_engine::TxEngine;
use slimchain_utils::{config::Config, init_tracing, path::binary_directory, tx_engine_threads};
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::signal;

cfg_if::cfg_if! {
    if #[cfg(all(feature = "simple", feature = "tee"))] {
        compile_error!("Only one of `simple` and `tee` can be enabled.");
    } else if #[cfg(feature = "simple")] {
        use slimchain_common::tx::SignedTx as Tx;
        use slimchain_tx_engine_simple::SimpleTxEngineWorker;

        fn create_tx_engine(_cfg: &Config, _enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
            Ok(TxEngine::new(tx_engine_threads(), || {
                let mut rng = rand::thread_rng();
                let keypair = slimchain_common::ed25519::Keypair::generate(&mut rng);
                Box::new(SimpleTxEngineWorker::new(keypair))
            }))
        }
    } else if #[cfg(feature = "tee")] {
        use slimchain_tee_sig::TEESignedTx as Tx;
        use slimchain_tx_engine_tee::{TEEConfig, TEETxEngineWorkerFactory};

        fn create_tx_engine(cfg: &Config, enclave: &Option<PathBuf>) -> Result<TxEngine<Tx>> {
            let tee_cfg: TEEConfig = cfg.get("tee")?;
            let factory = match enclave {
                Some(enclave) => TEETxEngineWorkerFactory::new(tee_cfg, enclave)?,
                None => TEETxEngineWorkerFactory::use_enclave_in_the_same_dir(tee_cfg)?,
            };
            TEETxEngineWorkerFactory::use_test(cfg.get("tee").unwrap()).unwrap();
            Ok(TxEngine::new(tx_engine_threads(), || factory.worker()))
        }
    } else {
        compile_error!("Require to enable either `simple` or `tee` feature.");
    }
}

macro_rules! run_swarmer {
    ($swarmer: expr, $net_cfg: expr) => {{
        let mut swarmer = $swarmer;
        let listen_addr = swarmer.listen_on_str(&$net_cfg.listen).await?;
        let peer_cfg = PeerConfig::new(swarmer.peer_id().clone(), listen_addr);
        peer_cfg.print_config_msg();
        let ctrl = swarmer.spawn();
        info!("Press Ctrl-C to quit.");
        signal::ctrl_c().await?;
        info!("Quitting.");
        ctrl.shutdown().await?;
    }};
}

#[derive(Debug, StructOpt)]
struct Opts {
    /// Path to the config.toml file.
    #[structopt(short, long, parse(from_os_str))]
    config: Option<PathBuf>,

    /// Path to the enclave file.
    #[structopt(short, long, parse(from_os_str))]
    enclave: Option<PathBuf>,

    /// Path to the data directory.
    #[structopt(short, long, parse(from_os_str))]
    data: Option<PathBuf>,

    /// Path to the metrics file.
    #[structopt(short, long, parse(from_os_str))]
    metrics: Option<PathBuf>,

    /// Change log level.
    #[structopt(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_backtrace::install();
    let opts = Opts::from_args();
    let bin_dir = binary_directory()?;

    let _guard = {
        let metrics = opts.metrics.unwrap_or_else(|| bin_dir.join("metrics.log"));
        let log_level = opts
            .log_level
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("info");
        init_tracing(log_level, &metrics)?
    };

    let cfg = if let Some(config) = opts.config {
        info!("Load config from {}.", config.display());
        Config::load(&config)?
    } else {
        Config::load_in_the_same_dir()?
    };

    let role: Role = cfg.get("role")?;
    info!("Role: {}", role);
    let chain_cfg: ChainConfig = cfg.get("chain")?;
    info!("Chain Cfg: {:#?}", chain_cfg);
    let net_cfg: NetworkConfig = cfg.get("network")?;

    let db = DB::open_or_create_in_dir(&opts.data.unwrap_or(bin_dir), role)?;

    match chain_cfg.consensus {
        Consensus::PoW => {
            use slimchain_network::behaviors::pow::*;

            match role {
                Role::Client => {
                    let behavior = ClientBehavior::<Tx>::new(db, &chain_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    run_swarmer!(swarmer, net_cfg);
                }
                Role::Miner => {
                    let miner_cfg: MinerConfig = cfg.get("miner")?;
                    info!("Miner Cfg: {:#?}", miner_cfg);
                    let behavior = MinerBehavior::<Tx>::new(db, &chain_cfg, &miner_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    run_swarmer!(swarmer, net_cfg);
                }
                Role::Storage(shard_id) => {
                    let engine = create_tx_engine(&cfg, &opts.enclave)?;
                    let behavior =
                        StorageBehavior::<Tx>::new(db, engine, shard_id, &chain_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    run_swarmer!(swarmer, net_cfg);
                }
            }
        }
        Consensus::Raft => {
            todo!();
        }
    }

    Ok(())
}
