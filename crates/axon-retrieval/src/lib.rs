//! Target pipeline crate skeleton for `axon-retrieval`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod citation;
pub mod context;
pub mod engine;
pub mod filter;
pub mod graph;
pub mod memory;
pub mod plan;
pub mod query;
pub mod rank;
pub mod testing;

pub const CRATE_NAME: &str = "axon-retrieval";
