//! Retrieve-by-URL: fetch a document's full stored content through a retrieval-owned port.
//!
//! `FullDocumentStore` keeps orchestration independent of the concrete vector
//! database. Composition crates provide concrete storage adapters. Part of the
//! issue #298 cutover: this is the `retrieve`
//! slice of the "query, search, retrieve, and the retrieval part of ask share
//! this engine" boundary described in this crate's `CLAUDE.md`.

use async_trait::async_trait;
use axon_api::source::ApiError;

type Result<T> = std::result::Result<T, ApiError>;

pub const MODULE_NAME: &str = "retrieve";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveVariantError {
    pub url: String,
    pub error: String,
}

/// Retrieval-owned full-document result, independent of a concrete vector DB.
#[derive(Debug, Clone, Default)]
pub struct RetrievedDocument {
    pub requested_url: String,
    pub matched_url: Option<String>,
    pub chunk_count: usize,
    pub max_points: usize,
    pub truncated: bool,
    pub variant_errors: Vec<RetrieveVariantError>,
    pub content: String,
}

#[async_trait]
pub trait FullDocumentStore: Send + Sync {
    async fn retrieve_full_document(
        &self,
        collection: &str,
        target: &str,
        max_points: Option<usize>,
    ) -> Result<RetrievedDocument>;
}

/// Fetch every stored chunk for `target` (trying canonical URL variants) and
/// render it into one document's markdown/text.
///
/// Returns `Ok` with empty content when `target` is simply not indexed; only a
/// transport-level failure across every URL variant produces `Err`.
pub async fn retrieve_document<S: FullDocumentStore + ?Sized>(
    store: &S,
    collection: &str,
    target: &str,
    max_points: Option<usize>,
) -> Result<RetrievedDocument> {
    store
        .retrieve_full_document(collection, target, max_points)
        .await
}

#[cfg(test)]
#[path = "retrieve_tests.rs"]
mod tests;
