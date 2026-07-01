//! Target pipeline crate skeleton for `axon-embedding`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod batch;
pub mod capability;
pub mod fake;
pub mod openai_compat;
pub mod provider;
pub mod reservation;
pub mod tei;
pub mod testing;

pub const CRATE_NAME: &str = "axon-embedding";
