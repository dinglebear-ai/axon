//! Target pipeline crate skeleton for `axon-document`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod chunk;
pub mod chunk_router;
pub mod code;
pub mod markdown;
pub mod metadata;
pub mod prepared;
pub mod preparer;
pub mod profile;
pub mod schema;
pub mod session;
pub mod testing;
pub mod text;
pub mod transcript;

pub const CRATE_NAME: &str = "axon-document";
