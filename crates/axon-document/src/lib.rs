//! Canonical document preparation for Axon's unified source pipeline.
//!
//! `DocumentPreparer` normalizes acquired source documents into deterministic
//! chunks and parse facts. Publication is owned separately by `axon-vectors`;
//! orchestration is provided by `axon-services`.

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
pub use preparer::{DocumentPreparer, DocumentPreparerConfig};
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
