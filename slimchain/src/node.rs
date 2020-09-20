use serde::{Deserialize, Serialize};
use slimchain_chain::{
    config::{ChainConfig, MinerConfig},
    consensus::Consensus,
    db::DB,
    role::Role,
};
use slimchain_common::{error::Result, tx::TxTrait};
use slimchain_network::{config::NetworkConfig, control::Swarmer};
use slimchain_tx_engine::TxEngine;
use slimchain_utils::{config::Config, init_tracing, path::binary_directory};
use std::path::PathBuf;
use structopt::StructOpt;

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

pub async fn node_main<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static>(
    create_tx_engine: impl FnOnce(&Config, &Option<PathBuf>) -> Result<TxEngine<Tx>>,
) -> Result<()> {
    color_backtrace::install();
    let opts = Opts::from_args();
    let bin_dir = binary_directory()?;

    let _guard = {
        let metrics = opts.metrics.unwrap_or_else(|| bin_dir.join("metrics.log"));
        let log_level = opts.log_level.as_deref().unwrap_or("info");
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
                    swarmer.app_run(&net_cfg.listen).await?;
                }
                Role::Miner => {
                    let miner_cfg: MinerConfig = cfg.get("miner")?;
                    info!("Miner Cfg: {:#?}", miner_cfg);
                    let behavior = MinerBehavior::<Tx>::new(db, &chain_cfg, &miner_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    swarmer.app_run(&net_cfg.listen).await?;
                }
                Role::Storage(shard_id) => {
                    let engine = create_tx_engine(&cfg, &opts.enclave)?;
                    let behavior =
                        StorageBehavior::<Tx>::new(db, engine, shard_id, &chain_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    swarmer.app_run(&net_cfg.listen).await?;
                }
            }
        }
        Consensus::Raft => {
            todo!();
        }
    }

    Ok(())
}