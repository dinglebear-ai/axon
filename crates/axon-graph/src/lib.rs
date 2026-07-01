//! Target pipeline crate skeleton for `axon-graph`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod authority;
pub mod candidate;
pub mod edge;
pub mod evidence;
pub mod merge;
pub mod migration;
pub mod node;
pub mod query;
pub mod sqlite;
pub mod store;
pub mod testing;

pub const CRATE_NAME: &str = "axon-graph";
