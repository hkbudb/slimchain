use slimchain_common::error::{Error, Result};
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub mod config;
pub mod contract;
pub mod metrics;
pub mod path;

pub use chrono;
pub use toml;
pub use tracing;

pub fn init_tracing(default_level: &str, metrics_file: &Path) -> Result<metrics::Guard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("slimchain={}", default_level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(Error::msg)?;
    metrics::init_metrics_subscriber_using_file(metrics_file)
}

pub fn init_tracing_for_test() -> Option<metrics::Guard> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("slimchain=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();
    metrics::init_metrics_subscriber(std::io::stdout()).ok()
}

pub fn tx_engine_threads() -> usize {
    if let Some(t) = std::env::var("TX_ENGINE_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        return t;
    }

    let cpus = num_cpus::get();
    if cpus == 1 {
        cpus
    } else {
        cpus - 1
    }
}
