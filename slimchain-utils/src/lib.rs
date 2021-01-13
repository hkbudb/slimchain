#[macro_use]
pub extern crate tracing;

use slimchain_common::error::{Error, Result};
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub mod config;
pub mod contract;
pub mod metrics;
pub mod ordered_stream;
pub mod path;

pub use chrono;
pub use toml;

pub fn init_tracing_subscriber(default_level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("slimchain={},warn", default_level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(Error::msg)
}

pub fn init_tracing(default_level: &str, metrics_file: &Path) -> Result<metrics::Guard> {
    init_tracing_subscriber(default_level)?;
    metrics::init_metrics_subscriber_using_file(metrics_file)
}

pub fn init_tracing_for_test() -> Option<metrics::Guard> {
    init_tracing_subscriber("info").ok();
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
