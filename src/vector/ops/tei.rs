mod pipeline;
mod prepare;
pub(crate) mod qdrant_store;
mod tei_client;
mod tei_manifest;
#[cfg(test)]
#[path = "tei_tests.rs"]
mod tests;
mod text_embed;

#[cfg(test)]
pub(crate) use tei_client::QUERY_INSTRUCTION;
#[cfg(test)]
pub(crate) use tei_client::prepend_query_instruction;
pub(crate) use tei_client::{EmbedInput, tei_embed_typed};

// Re-export the embed API for crate callers.
pub(crate) use text_embed::embed_prepared_docs;
pub use text_embed::{embed_path_native, embed_path_native_with_progress};

use crate::vector::ops::sparse;
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
    /// Optional vertical-extractor identifier (e.g. `"docs"`, `"github-issue"`).
    /// `None` for generic scrape/embed paths — leave absent from payload rather
    /// than writing a placeholder. See bead `axon_rust-lu6a`.
    pub(crate) extractor_name: Option<String>,
    /// Optional structured-data attached at page level (JSON-LD / __NEXT_DATA__ /
    /// SvelteKit). `None` for paths that don't run the structured pass. When set,
    /// these payload fields land on every chunk so retrieval can filter by
    /// `structured_kind` / `structured_type` and dedup by `structured_id`.
    /// The full payload lives in `structured_blob` (capped to
    /// `cfg.structured_data_max_bytes` by the caller). See bead `axon_rust-xvu9`.
    pub(crate) structured: Option<StructuredPayload>,
    /// Optional per-chunk payload overrides, positionally parallel to `chunks`.
    /// When `chunk_extra[i]` is present, its object keys are merged into chunk
    /// `i`'s Qdrant payload on top of the doc-level `extra` (chunk keys win,
    /// reserved system keys excepted). Empty for the common case. GitHub code
    /// ingest uses this to attach per-chunk `symbol_*` / `code_line_*` metadata
    /// while still grouping a file's chunks into a single `PreparedDoc` (P-H1),
    /// so the symbol-boost retrieval signal survives the per-file batching.
    pub(crate) chunk_extra: Vec<serde_json::Value>,
    /// Post-upsert maintenance marker for local file embeds that used to create
    /// one URL per line fragment (`file://...#Lx-Ly`). When present, the pipeline
    /// deletes only those legacy fragment URLs after the replacement file URL has
    /// been durably upserted.
    pub(crate) local_legacy_fragment_url: Option<String>,
}

impl PreparedDoc {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_planned_chunks(
        url: String,
        domain: String,
        chunks: Vec<String>,
        source_type: impl Into<String>,
        content_type: &'static str,
        title: Option<String>,
        extra: Option<serde_json::Value>,
        extractor_name: Option<String>,
        structured: Option<StructuredPayload>,
        chunk_extra: Vec<serde_json::Value>,
    ) -> Self {
        Self {
            url,
            domain,
            chunks,
            source_type: source_type.into(),
            content_type,
            title,
            extra,
            extractor_name,
            structured,
            chunk_extra,
            local_legacy_fragment_url: None,
        }
    }

    pub(super) fn with_local_legacy_fragment_cleanup(mut self) -> Self {
        self.local_legacy_fragment_url = Some(self.url.clone());
        self
    }
}

/// Per-page structured-data payload attached to every chunk of a doc.
///
/// Produced by `core::structured::extract_all()` at scrape time and reduced
/// to a single dominant entry for payload storage:
/// - `kind` = which extractor produced this (`"jsonld"` | `"next_data"` | `"sveltekit"`)
/// - `schema_type` = top-level `@type` of the dominant entry, if any
/// - `schema_id` = top-level `@id` for cross-page dedup, if any
/// - `blob` = the full serialized pass (JSON value), capped by caller before
///   construction
#[derive(Debug, Clone)]
pub(crate) struct StructuredPayload {
    pub(crate) kind: &'static str,
    pub(crate) schema_type: Option<String>,
    pub(crate) schema_id: Option<String>,
    pub(crate) blob: serde_json::Value,
}

impl StructuredPayload {
    /// Reduce a structured pass to a single payload, enforcing the
    /// `max_bytes` cap on `structured_blob` (bead axon_rust-xvu9).
    ///
    /// Returns `None` when the pass is empty or when the serialized blob
    /// exceeds `max_bytes` (oversized payloads are DROPPED, not truncated —
    /// truncation would yield invalid JSON, and silently shipping a
    /// half-payload would defeat downstream filtering).
    ///
    /// `kind`, `schema_type` (top-level `@type`), and `schema_id`
    /// (top-level `@id`) are taken from the dominant entry as returned by
    /// [`crate::core::structured::StructuredDataPass::dominant`].
    pub(crate) fn from_pass(
        pass: &crate::core::structured::StructuredDataPass,
        max_bytes: usize,
    ) -> Option<Self> {
        let (kind, value) = pass.dominant()?;
        // Measure the serialized footprint once. Clone the original Value
        // into the payload — no need to round-trip bytes -> Value just to
        // get back the same data. (review: avoid double serialization)
        let blob_bytes = serde_json::to_vec(value).ok()?;
        if blob_bytes.len() > max_bytes {
            return None;
        }
        let schema_type = crate::core::structured::schema_type_of(value);
        let schema_id = crate::core::structured::schema_id_of(value);
        Some(Self {
            kind,
            schema_type,
            schema_id,
            blob: value.clone(),
        })
    }
}

/// Build a Qdrant point JSON value with the correct vector format for the collection mode.
///
/// - `Named`: emits `"vector": {"dense": [...], "bm42": {"indices": [...], "values": [...]}}`
/// - `Unnamed`: emits `"vector": [...]` (flat dense vector)
pub(crate) fn build_point(
    point_id: uuid::Uuid,
    vecv: Vec<f32>,
    chunk: &str,
    payload: serde_json::Value,
    mode: VectorMode,
) -> serde_json::Value {
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector_for_indexing(chunk);
            serde_json::json!({
                "id": point_id.to_string(),
                "vector": {
                    "dense": vecv,
                    "bm42": sv
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
