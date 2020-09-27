#[macro_use]
extern crate tracing;

use baseline::{
    config::{ChainConfig, Consensus, MinerConfig, Role},
    db::DB,
    init_tracing,
};
use slimchain_common::error::{bail, Result};
use slimchain_network::{config::NetworkConfig, control::Swarmer};
use slimchain_utils::{config::Config, path::binary_directory};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opts {
    /// Path to the config.toml file.
    #[structopt(short, long, parse(from_os_str))]
    config: Option<PathBuf>,

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
            use baseline::config::PoWConfig;
            use baseline::network::pow::*;

            let pow_cfg: PoWConfig = cfg.get("pow").unwrap_or_default();
            pow_cfg.install_as_global()?;

            match role {
                Role::Client => {
                    let behavior = ClientBehavior::new(db, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    swarmer.app_run(&net_cfg.listen).await?;
                }
                Role::Miner => {
                    let miner_cfg: MinerConfig = cfg.get("miner")?;
                    info!("Miner Cfg: {:#?}", miner_cfg);
                    let behavior = MinerBehavior::new(db, &miner_cfg, &net_cfg)?;
                    let swarmer = Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior)?;
                    swarmer.app_run(&net_cfg.listen).await?;
                }
                _ => bail!("Role can only be client or miner."),
            }
        }
        Consensus::Raft => {
            todo!();
        }
    }

    Ok(())
}
