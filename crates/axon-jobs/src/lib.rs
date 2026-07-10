pub mod backend;
pub mod boundary;
pub mod cancel;
pub mod config_snapshot;
pub mod crawl;
pub mod embed;
pub mod error;
pub mod extract;
mod fake_store;
pub mod freshness;
pub mod ingest;
pub mod limits;
pub mod migrations;
pub mod ops;
pub mod query;
pub mod runtime;
mod service_job_conv;
pub mod state_machine;
pub mod status;
pub mod store;
pub mod store_inventory;
pub(crate) mod tx;
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
