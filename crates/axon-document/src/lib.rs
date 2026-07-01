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

pub use chunk_router::ChunkRouter;
pub use prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
pub use preparer::DocumentPreparer;
pub use profile::ChunkingProfile;

#[cfg(test)]
#[path = "chunk_router_tests.rs"]
mod chunk_router_tests;

#[cfg(test)]
#[path = "preparer_tests.rs"]
mod preparer_tests;

pub const CRATE_NAME: &str = "axon-document";
