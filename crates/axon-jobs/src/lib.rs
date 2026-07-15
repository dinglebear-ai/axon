#![allow(clippy::result_large_err, clippy::too_many_arguments)]

pub mod boundary;
pub mod config_snapshot;
pub mod config_snapshot_store;
pub mod error;
mod fake_store;
pub mod freshness;
pub mod ingest;
pub mod limits;
pub mod migrations;
pub mod runtime;
pub mod state_machine;
pub mod status;
pub mod store;
pub mod unified;
pub(crate) mod unified_codec;
pub mod watch;
pub mod watch_store;
pub mod workers;

pub use runtime::SqliteJobBackend;

#[cfg(test)]
#[path = "state_machine_tests.rs"]
mod state_machine_tests;

#[cfg(test)]
#[path = "freshness_tests.rs"]
mod freshness_tests;

#[cfg(test)]
#[path = "provider_cooling_tests.rs"]
mod provider_cooling_tests;
