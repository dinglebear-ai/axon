mod pipeline;
mod prepare;
pub(crate) mod qdrant_store;
mod tei_client;
mod tei_manifest;
#[cfg(test)]
mod tests;
mod text_embed;

pub(crate) use tei_client::tei_embed;

// Re-export public API so callers outside this module see no change.
pub(crate) use text_embed::embed_prepared_docs;
pub use text_embed::{embed_path_native, embed_path_native_with_progress};

use crate::crates::vector::ops::sparse;
use qdrant_store::VectorMode;

#[derive(Debug, Clone, Copy)]
pub struct EmbedSummary {
    pub docs_embedded: usize,
    pub docs_failed: usize,
    pub chunks_embedded: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct EmbedProgress {
    pub docs_total: usize,
    pub docs_completed: usize,
    pub chunks_embedded: usize,
}

#[derive(Debug)]
pub(crate) struct PreparedDoc {
    pub(crate) url: String,
    pub(crate) domain: String,
    pub(crate) chunks: Vec<String>,
    /// "embed" for crawl path, "github"/"reddit"/"youtube"/"sessions" for ingest.
    pub(crate) source_type: String,
    /// "markdown" for crawl path, "text" for ingest sources.
    pub(crate) content_type: &'static str,
    pub(crate) title: Option<String>,
    /// Source-specific metadata fields (gh_*, reddit_*, yt_*).
    pub(crate) extra: Option<serde_json::Value>,
}

/// Build a Qdrant point JSON value with the correct vector format for the collection mode.
///
/// - `Named`: emits `"vector": {"dense": [...], "bm42": {"indices": [...], "values": [...]}}`
/// - `Unnamed`: emits `"vector": [...]` (flat dense vector)
pub(super) fn build_point(
    point_id: uuid::Uuid,
    vecv: Vec<f32>,
    chunk: &str,
    payload: serde_json::Value,
    mode: VectorMode,
) -> serde_json::Value {
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector(chunk);
            serde_json::json!({
                "id": point_id.to_string(),
                "vector": {
                    "dense": vecv,
                    "bm42": sv.to_json()
                },
                "payload": payload,
            })
        }
        VectorMode::Unnamed => {
            serde_json::json!({
                "id": point_id.to_string(),
                "vector": vecv,
                "payload": payload,
            })
        }
    }
}

#[cfg(test)]
pub(super) fn build_point_for_test(
    dense: Vec<f32>,
    chunk: &str,
    url: &str,
    idx: usize,
    mode: VectorMode,
) -> serde_json::Value {
    let point_id = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("{url}:{idx}").as_bytes(),
    );
    let payload = serde_json::json!({"url": url, "chunk_text": chunk, "chunk_index": idx});
    build_point(point_id, dense, chunk, payload, mode)
}
