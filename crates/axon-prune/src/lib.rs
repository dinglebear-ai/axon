//! Target pipeline crate skeleton for `axon-prune`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod debt;
pub mod dedupe;
pub mod executor;
pub mod generation;
pub mod orphan;
pub mod plan;
pub mod receipt;
pub mod safety;
pub mod testing;

pub const CRATE_NAME: &str = "axon-prune";
