//! Durable memory subsystem for the axon pipeline (#298).
//!
//! `axon-memory` owns memory lifecycle, scoring/decay, review, reinforcement,
//! supersession, and contradiction handling. DTOs live in `axon-api`
//! (`source::memory`); this crate implements behavior.
//!
//! - [`store::MemoryStore`] — the async store boundary (+ `FakeMemoryStore`).
//! - [`sqlite::SqliteMemoryStore`] — the real SQLite-backed implementation.
//! - [`decay`] — the contract score/decay/reinforcement math.
//! - [`migration`] — the in-crate SQLite schema.

// The store returns `axon_api::source::ApiError` by value — the pipeline's
// shared contract error type. It is a large enum by design; boxing it here
// would diverge from every other DTO boundary, so we allow the lint crate-wide
// (matching the `#[allow(clippy::result_large_err)]` convention in axon-web).
#![allow(clippy::result_large_err)]

pub mod context;
pub mod decay;
pub mod graph;
pub mod link;
pub mod migration;
pub mod recall;
pub mod record;
pub mod review;
pub mod sqlite;
pub mod store;
pub mod testing;
pub mod vector;

pub const CRATE_NAME: &str = "axon-memory";

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "shared_pipeline_tests.rs"]
mod shared_pipeline_tests;
