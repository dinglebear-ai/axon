//! Durable job scheduling, execution, state transitions, and persistence.

#![allow(clippy::result_large_err, clippy::too_many_arguments)]

pub mod boundary;
pub mod config_snapshot;
pub mod config_snapshot_store;
pub mod embedding_cache_store;
pub mod error;
mod fake_store;
pub mod limits;
pub mod migrations;
pub mod runtime;
pub mod scheduler;
pub mod state_machine;
pub mod status;
pub mod store;
pub mod unified;
pub(crate) mod unified_codec;
pub mod watch_schedule;
pub mod watch_store;
pub mod workers;

pub use runtime::SqliteJobBackend;

#[cfg(test)]
#[path = "state_machine_tests.rs"]
mod state_machine_tests;

#[cfg(test)]
#[path = "provider_cooling_tests.rs"]
mod provider_cooling_tests;

#[cfg(test)]
#[path = "tx_tests.rs"]
mod tx_tests;

#[cfg(test)]
#[path = "security_error_memory_e2e_tests.rs"]
mod security_error_memory_e2e_tests;
