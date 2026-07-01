//! Target pipeline crate skeleton for `axon-memory`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

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

pub const CRATE_NAME: &str = "axon-memory";
