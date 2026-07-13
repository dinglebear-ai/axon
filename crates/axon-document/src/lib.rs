//! Target pipeline crate for `axon-document` (issue #298).
//!
//! Live, not marker-only: `DocumentPreparer` is wired into every source
//! family's vectorize path (feed/git/local/reddit/registry/sessions/web/
//! youtube in `axon-services`, plus `axon-memory` and `axon-vector`'s
//! `document_bridge`). `prepare_version` is currently the fixed literal
//! `"axon-document-pr8"` (see `preparer.rs`) — a placeholder pending a real
//! content-addressed or semantic versioning scheme, not yet load-bearing for
//! cache invalidation.

#![allow(clippy::result_large_err)]

pub mod boundary;
pub mod chunk;
pub mod chunk_router;
pub mod code;
pub mod markdown;
pub mod metadata;
mod parse;
pub mod prepared;
pub mod preparer;
pub mod profile;
pub mod schema;
pub mod session;
pub mod source_range;
pub mod structured_formats;
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

#[cfg(test)]
#[path = "local_source_tests.rs"]
mod local_source_tests;

pub const CRATE_NAME: &str = "axon-document";
