//! Target pipeline crate skeleton for `axon-vectors`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod collection;
pub mod filter;
pub mod health;
pub mod payload;
pub mod point;
pub mod qdrant;
pub mod query;
pub mod store;
pub mod testing;

pub const CRATE_NAME: &str = "axon-vectors";
