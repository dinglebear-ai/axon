//! Target pipeline retrieval boundary for PR9 vector/embedding scaffolding.
//!
//! Runtime RAG cutover stays in later issue #298 phases; this crate keeps the
//! retrieval fake private until shared wire DTOs move through `axon-api`.

#![allow(dead_code)]

pub mod citation;
pub mod context;
pub mod engine;
pub mod filter;
pub mod graph;
pub mod memory;
pub mod plan;
pub mod query;
pub mod rank;
pub(crate) mod testing;

pub const CRATE_NAME: &str = "axon-retrieval";

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;
