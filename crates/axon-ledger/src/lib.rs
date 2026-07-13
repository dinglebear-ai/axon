//! Target pipeline crate skeleton for `axon-ledger`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

#![allow(clippy::result_large_err)]

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

/// Number of most-recently-committed generations kept in the ledger before
/// `LedgerPrune` cleanup debt is recorded against the older ones.
///
/// Matches the documented retention policy for `source_generations`
/// (`docs/pipeline-unification/schemas/database-schema.md`:
/// `"retention": "last_2_committed_plus_active_cleanup_debt"`) — keep the
/// newly committed generation plus its immediate predecessor always. A
/// generation beyond this window is *also* skipped for one more publish cycle
/// while it still has other unresolved (non-`LedgerPrune`) cleanup debt —
/// vector/graph/memory — referencing it, matching the "...plus
/// active_cleanup_debt" half of that same retention string. See
/// `sqlite::generation::ledger_prune` (SQLite) and
/// `store::fake::cleanup::record_ledger_prune_cleanup_debt` (fake) for the
/// producers that apply this constant.
pub(crate) const LEDGER_GENERATION_RETENTION_COMMITTED: usize = 2;

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod sqlite_tests;
