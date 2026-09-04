//! Retrieve-by-URL: fetch a document's full stored content from Qdrant.
//!
//! Thin composition over `axon-vectors`' `QdrantVectorStore::retrieve_by_url`
//! plus `render_full_doc_from_points`, giving `axon-services::query::retrieve`
//! one call that returns both the raw result bookkeeping (URL-variant match,
//! truncation, per-variant errors) and the rendered full-document text —
//! mirroring legacy `axon-vector`'s `retrieve_result` shape without
//! reimplementing it. Part of the issue #298 cutover: this is the `retrieve`
//! slice of the "query, search, retrieve, and the retrieval part of ask share
//! this engine" boundary described in this crate's `CLAUDE.md`.

use axon_vectors::qdrant::{
    QdrantRetrieveByUrlResult, QdrantVectorStore, render_full_doc_from_points,
};
use axon_vectors::store::Result;

pub const MODULE_NAME: &str = "retrieve";

/// A [`QdrantRetrieveByUrlResult`] plus its rendered full-document text.
#[derive(Debug, Clone, Default)]
pub struct RetrievedDocument {
    pub result: QdrantRetrieveByUrlResult,
    pub content: String,
}

/// Fetch every stored chunk for `target` (trying canonical URL variants) and
/// render it into one document's markdown/text.
///
/// Returns `Ok` with empty `result.points`/`content` when `target` is simply
/// not indexed; only a transport-level failure across every URL variant
/// produces `Err`.
pub async fn retrieve_document(
    store: &QdrantVectorStore,
    collection: &str,
    target: &str,
    max_points: Option<usize>,
) -> Result<RetrievedDocument> {
    let result = store
        .retrieve_by_url(collection, target, max_points)
        .await?;
    let content = render_full_doc_from_points(&result.points);
    Ok(RetrievedDocument { result, content })
}

#[cfg(test)]
#[path = "retrieve_tests.rs"]
mod tests;
