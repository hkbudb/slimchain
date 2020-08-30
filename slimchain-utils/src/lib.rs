pub mod config;
pub mod contract;
pub mod metrics;
pub mod path;

pub use chrono;
pub use toml;
pub use tracing;

pub fn init_tracing_for_test() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();
    metrics::init_metrics_subscriber(std::io::stdout()).ok();
}
