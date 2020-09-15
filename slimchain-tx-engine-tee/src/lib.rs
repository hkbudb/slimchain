#![allow(clippy::missing_safety_doc)]

#[macro_use]
extern crate tracing;

pub(crate) mod config;
pub(crate) mod ecall;
pub(crate) mod engine;
pub(crate) mod intel_api;
pub(crate) mod ocall;

pub use config::TEEConfig;
pub use engine::{TEETxEngineWorker, TEETxEngineWorkerFactory};

#[cfg(sim_enclave)]
pub const fn is_sim_mode() -> bool {
    true
}

#[cfg(not(sim_enclave))]
pub const fn is_sim_mode() -> bool {
    false
}

#[cfg(test)]
mod tests;
