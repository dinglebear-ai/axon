//! Compatibility shim: query/ask/evaluate result DTOs now live in `axon-api`.
//!
//! These transport-neutral result contracts moved to `axon_api::result` as
//! part of the workspace crate extraction (Cycle 1 break, epic axon_rust-23dw).
//! Existing `crate::types::*` call sites keep resolving through this
//! re-export, and the ask-explain re-export is preserved via the
//! `axon_core::ask_explain` shim.

pub use axon_api::result::*;
pub use axon_core::ask_explain::*;
