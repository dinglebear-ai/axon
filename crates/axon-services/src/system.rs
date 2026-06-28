//! `system` service module — collection facets, diagnostics, status, dedupe.
//!
//! This module is the public entry point for the `axon` sources / domains /
//! stats / doctor / status / dedupe commands. Concrete implementations live in
//! the submodules below; everything that used to be exported from `system.rs`
//! before the split is re-exported here so external callers keep working
//! without any `use` changes.

mod collections;
mod dedupe;
mod doctor;
mod domains;
mod purge;
mod sources;
mod stats;
mod status;
mod watchdog;

pub use self::collections::{CollectionsError, collections, map_collections_payload};
pub use self::dedupe::dedupe;
pub use self::doctor::{doctor, map_doctor_payload};
pub use self::domains::{
    detailed_domains, domain_indexed, domains, map_domains_payload, summarize_detailed_domains,
    summarize_detailed_domains_limited,
};
pub use self::purge::purge;
pub use self::sources::{
    domain_sources_from_urls, map_sources_payload, normalize_domain_query, sources,
    sources_for_domain, sources_schema_version_breakdown, sources_with_breakdown,
};
pub use self::stats::{map_stats_payload, print_stats_human, stats};
pub use self::status::{
    StatusJobs, build_status_payload, build_status_payload_with_errors,
    build_status_payload_with_errors_and_sqlite, full_status, load_status_jobs,
    sqlite_status_error,
};

/// Error type for payload parsing failures shared across the `system`
/// submodules (sources, domains, etc.).
#[derive(Debug, thiserror::Error)]
#[error("payload parse error: {0}")]
pub struct PayloadParseError(String);

impl PayloadParseError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}
