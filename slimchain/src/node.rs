use serde::{Deserialize, Serialize};
use slimchain_chain::{
    config::{ChainConfig, MinerConfig},
    consensus::Consensus,
    db::DB,
    role::Role,
};
use slimchain_common::{
    error::{bail, Context as _, Result},
    tx::TxTrait,
};
use slimchain_network::p2p::control::Swarmer;
use slimchain_tx_engine::TxEngine;
use slimchain_utils::{config::Config, init_tracing, path::binary_directory};
use std::{path::PathBuf, time::Duration};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(version = git_version::git_version!(prefix = concat!(env!("CARGO_PKG_VERSION"), " ("), suffix = ")", fallback = "unknown"))]
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

    /// Enable RocksDB statistics.
    #[structopt(long)]
    db_statistics: bool,
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

    let db = DB::open_or_create_in_dir(&opts.data.unwrap_or(bin_dir), role, opts.db_statistics)?;

    match chain_cfg.consensus {
        Consensus::PoW => {
            use slimchain_chain::config::PoWConfig;
            use slimchain_network::{behavior::pow::*, p2p::config::NetworkConfig};

            let net_cfg: NetworkConfig = cfg.get("network")?;

            let pow_cfg: PoWConfig = cfg.get("pow").unwrap_or_default();
            info!("PoW initial difficulty: {}", pow_cfg.init_diff);
            pow_cfg.install_as_global()?;

            match role {
                Role::Client => {
                    let behavior = ClientBehavior::<Tx>::new(db, &chain_cfg, &net_cfg).await?;
                    let swarmer =
                        Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior).await?;
                    let mut ctrl = swarmer.spawn_app(&net_cfg.listen).await?;
                    let _miner_peer_id = ctrl
                        .call_with_sender(|swarm, ret| {
                            swarm.behaviour_mut().discv_mut().find_random_peer_with_ret(
                                Role::Miner,
                                Duration::from_secs(60),
                                ret,
                            )
                        })
                        .await?
                        .context("Failed to find miner.")?;
                    ctrl.run_until_interrupt().await?;
                }
                Role::Miner => {
                    let miner_cfg: MinerConfig = cfg.get("miner")?;
                    info!("Miner Cfg: {:#?}", miner_cfg);
                    let behavior =
                        MinerBehavior::<Tx>::new(db, &chain_cfg, &miner_cfg, &net_cfg).await?;
                    let swarmer =
                        Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior).await?;
                    let ctrl = swarmer.spawn_app(&net_cfg.listen).await?;
                    ctrl.run_until_interrupt().await?;
                }
                Role::Storage(shard_id) => {
                    let engine = create_tx_engine(&cfg, &opts.enclave)?;
                    let behavior =
                        StorageBehavior::<Tx>::new(db, engine, shard_id, &chain_cfg, &net_cfg)
                            .await?;
                    let swarmer =
                        Swarmer::new(net_cfg.keypair.to_libp2p_keypair(), behavior).await?;
                    let mut ctrl = swarmer.spawn_app(&net_cfg.listen).await?;
                    let _miner_peer_id = ctrl
                        .call_with_sender(|swarm, ret| {
                            swarm.behaviour_mut().discv_mut().find_random_peer_with_ret(
                                Role::Miner,
                                Duration::from_secs(60),
                                ret,
                            )
                        })
                        .await?
                        .context("Failed to find miner.")?;
                    ctrl.run_until_interrupt().await?;
                }
            }
        }
        Consensus::Raft => {
            use slimchain_network::{
                behavior::raft::{client::ClientNode, storage::StorageNode},
                http::config::{NetworkConfig, RaftConfig},
            };

            let net_cfg: NetworkConfig = cfg.get("network")?;
            let raft_cfg: RaftConfig = cfg.get("raft")?;

            match role {
                Role::Client => {
                    let miner_cfg: MinerConfig = cfg.get("miner")?;
                    info!("Miner Cfg: {:#?}", miner_cfg);
                    let mut client: ClientNode<Tx> =
                        ClientNode::new(db, &chain_cfg, &miner_cfg, &net_cfg, &raft_cfg).await?;
                    info!("Press Ctrl-C to quit.");
                    tokio::signal::ctrl_c().await?;
                    info!("Quitting.");
                    client.shutdown().await?;
                }
                Role::Storage(shard_id) => {
                    let engine = create_tx_engine(&cfg, &opts.enclave)?;
                    let mut storage =
                        StorageNode::new(db, engine, shard_id, &chain_cfg, &net_cfg).await?;
                    info!("Press Ctrl-C to quit.");
                    tokio::signal::ctrl_c().await?;
                    info!("Quitting.");
                    storage.shutdown().await?;
                }
                Role::Miner => {
                    bail!("Role cannot be miner.");
                }
            }
        }
    }

    Ok(())
}
