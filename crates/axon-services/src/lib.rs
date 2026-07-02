pub mod action_api;
pub mod artifacts;
pub mod brand;
pub mod client_contract;
pub mod code_search_watch;
pub mod config;
pub mod context;
pub mod crawl;
pub mod crawl_sync;
pub mod debug;
pub mod diff;
pub mod document;
pub mod embed;
pub mod endpoints;
pub mod events;
pub mod extract;
pub mod freshness;
pub mod ingest;
pub mod jobs;
pub(crate) mod local_source;
pub mod map;
pub mod memory;
pub mod migrate;
pub mod mobile_sessions;
pub mod query;
pub mod refresh;
pub mod runtime;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod search_crawl;
pub mod sessions;
pub mod setup;
pub mod source_jobs;
#[allow(dead_code)]
pub(crate) mod source_spike;
pub mod summarize;
pub mod sync;
pub mod system;
pub mod transport;
pub mod types;
pub mod watch;
#[allow(dead_code)]
pub(crate) mod web_source;

#[cfg(test)]
#[path = "client_contract_tests.rs"]
mod client_contract_tests;
#[cfg(test)]
#[path = "freshness_tests.rs"]
mod freshness_tests;
#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
#[cfg(test)]
pub(crate) mod test_support;
