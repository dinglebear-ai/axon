//! Compatibility shim: ask-explain trace DTOs now live in the `axon-api` crate.
//!
//! These transport-neutral types moved to `axon_api::explain` as part of the
//! workspace crate extraction (Cycle 1 break, epic axon_rust-23dw). Existing
//! `crate::ask_explain::*` call sites keep resolving through this
//! re-export. `core` depending on `axon-api` is an allowed downward edge.

pub use axon_api::explain::*;
