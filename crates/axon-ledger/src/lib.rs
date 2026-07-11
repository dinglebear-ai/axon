//! Target pipeline crate skeleton for `axon-ledger`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod cleanup_debt;
pub mod diff;
pub mod document_status;
pub mod generation;
pub mod item;
pub mod lease;
pub mod listing;
pub mod manifest;
pub mod migration;
pub mod source;
pub mod sqlite;
pub mod store;
pub mod testing;
pub mod transaction;
pub mod validation;

pub const CRATE_NAME: &str = "axon-ledger";

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod sqlite_tests;
