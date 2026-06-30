//! Axon transport-neutral API contracts.
//!
//! This crate owns the shared DTOs that cross the entry-point boundary (CLI,
//! MCP, HTTP, generated clients). It deliberately has no dependency on the Axon
//! application crates (services, vector, jobs, crawl, mcp, web), so both the
//! retrieval/vector layer and the services façade can depend on it without
//! forming a cycle.
//!
//! Seeded with the ask/query/evaluate result contracts and the ask-explain
//! trace types (the former `services::types::service::query` and
//! `core::ask_explain` modules), which break the historical
//! `vector` ↔ `services` dependency cycle (inventory §8.1 Cycle 1). Route and
//! request DTOs fold in as bead `axon_rust-23dw.2` continues.

pub mod contract;
pub mod diff;
pub mod explain;
pub mod ingest;
pub mod job_dto;
pub mod job_progress;
pub mod job_status;
pub mod mcp_schema;
pub mod purge;
pub mod result;
pub mod service_job;
pub mod source;

pub use explain::*;
pub use purge::*;
pub use result::*;
pub use source::*;

#[cfg(test)]
mod contract_tests;
