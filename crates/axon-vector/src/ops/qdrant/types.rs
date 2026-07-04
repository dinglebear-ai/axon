use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QdrantPayload {
    #[serde(default)]
    pub url: String,
    /// New unified-pipeline points carry `item_canonical_uri` (the source
    /// canonical URI) instead of a bare `url`. Retrieval falls back to this when
    /// `url` is empty so new-pipeline content is not skipped as url-less. Legacy
    /// points have `url` and no `item_canonical_uri`, so this stays empty for
    /// them and behavior is unchanged.
    #[serde(default)]
    pub item_canonical_uri: String,
    #[serde(default)]
    pub chunk_text: String,
    #[serde(default)]
    pub text: String,
    pub chunk_index: Option<i64>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub git_content_kind: Option<String>,
    #[serde(default)]
    pub git_file_path: Option<String>,
    #[serde(default)]
    pub code_file_path: Option<String>,
    #[serde(default)]
    pub code_language: Option<String>,
    #[serde(default)]
    pub code_file_type: Option<String>,
    #[serde(default)]
    pub code_is_test: Option<bool>,
    #[serde(default)]
    pub code_line_start: Option<u32>,
    #[serde(default)]
    pub code_line_end: Option<u32>,
    #[serde(default)]
    pub code_chunking_method: Option<String>,
    #[serde(default)]
    pub symbol_name: Option<String>,
    #[serde(default)]
    pub symbol_kind: Option<String>,
    #[serde(default)]
    pub symbol_extraction_status: Option<String>,
    #[serde(default, rename = "type")]
    pub memory_type: Option<String>,
    #[serde(default, rename = "title")]
    pub memory_title: Option<String>,
    #[serde(default, rename = "body")]
    pub memory_body: Option<String>,
    #[serde(default, rename = "project")]
    pub memory_project: Option<String>,
    #[serde(default, rename = "repo")]
    pub memory_repo: Option<String>,
    #[serde(default, rename = "file")]
    pub memory_file: Option<String>,
    #[serde(default, rename = "workspace")]
    pub memory_workspace: Option<String>,
    #[serde(default, rename = "git_branch")]
    pub memory_git_branch: Option<String>,
    #[serde(default, rename = "git_commit")]
    pub memory_git_commit: Option<String>,
    #[serde(default, rename = "git_dirty")]
    pub memory_git_dirty: Option<bool>,
    #[serde(default, rename = "cwd")]
    pub memory_cwd: Option<String>,
    #[serde(default, rename = "confidence")]
    pub memory_confidence: Option<f64>,
    #[serde(default, rename = "status")]
    pub memory_status: Option<String>,
    #[serde(default, rename = "created_at")]
    pub memory_created_at: Option<i64>,
    #[serde(default, rename = "updated_at")]
    pub memory_updated_at: Option<i64>,
    #[serde(default, rename = "last_seen_at")]
    pub memory_last_seen_at: Option<i64>,
    #[serde(default, rename = "access_count")]
    pub memory_access_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantPoint {
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub payload: QdrantPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveVariantError {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct QdrantRetrieveByUrlResult {
    pub url_match: String,
    pub points: Vec<QdrantPoint>,
    pub max_points: usize,
    pub malformed_points: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct DirectRetrieveResult {
    pub requested_url: String,
    pub matched_url: Option<String>,
    pub chunk_count: usize,
    pub content: String,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub variant_errors: Vec<RetrieveVariantError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantSearchHit {
    #[serde(default)]
    pub id: serde_json::Value,
    pub score: f64,
    #[serde(default)]
    pub payload: QdrantPayload,
}

/// Response from `/points/search` — `result` is a flat array.
#[derive(Debug, Deserialize)]
pub(crate) struct QdrantSearchResponse {
    #[serde(default)]
    pub(crate) result: Vec<QdrantSearchHit>,
}

/// Response from `/points/query` — `result` is `{"points": [...]}`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct QdrantQueryResult {
    #[serde(default)]
    pub(crate) points: Vec<QdrantSearchHit>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QdrantQueryResponse {
    #[serde(default)]
    pub(crate) result: QdrantQueryResult,
}

/// Response from `/points/query/batch` — `result` is a positionally-aligned
/// array of per-query result shapes (each identical to `/points/query`'s
/// `{"points": [...]}`). Used by [`qdrant_dual_search`] (bd axon_rust-j2c).
#[derive(Debug, Deserialize)]
pub(crate) struct QdrantBatchQueryResponse {
    #[serde(default)]
    pub(crate) result: Vec<QdrantQueryResult>,
}

pub(crate) const RETRIEVE_MAX_POINTS_CEILING: usize = 500;

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
