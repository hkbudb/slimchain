#[macro_use]
extern crate tracing;

pub mod behavior;
pub mod block_proposal;
pub mod network;
pub mod node;
pub mod snapshot;

use slimchain_common::error::{Error, Result};
use slimchain_utils::metrics;
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub fn init_tracing_subscriber(default_level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "slimchain={},baseline_stateful={},warp::reject=off,warn",
            default_level, default_level
        ))
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(Error::msg)
}

pub fn init_tracing(default_level: &str, metrics_file: &Path) -> Result<metrics::Guard> {
    init_tracing_subscriber(default_level)?;
    metrics::init_metrics_subscriber_using_file(metrics_file)
}
