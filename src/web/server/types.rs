use serde::{Deserialize, Serialize};

/// Hard limit on `/v1/ask` request bodies. Matches the existing 64 KiB cap used
/// by `dispatch_vector_search` so the web surface mirrors MCP behavior.
pub(super) const ASK_BODY_LIMIT: usize = 64 * 1024;
/// Reject ask queries longer than this (defense-in-depth above body cap).
pub(super) const ASK_QUERY_MAX_CHARS: usize = 16 * 1024;

#[derive(Serialize)]
pub(super) struct StateResponse {
    pub(super) setup_required: bool,
    pub(super) config_path: String,
}

#[derive(Deserialize)]
pub(super) struct LoginRequest {
    pub(super) password: String,
}

#[derive(Serialize)]
pub(super) struct LoginResponse {
    pub(super) ok: bool,
    pub(super) token: Option<String>,
}

#[derive(Serialize)]
pub(super) struct ConfigResponse {
    pub(super) path: String,
    pub(super) raw_toml: String,
    pub(super) restart_required: bool,
}

#[derive(Serialize)]
pub(super) struct SaveConfigResponse {
    pub(super) ok: bool,
    pub(super) restart_required: bool,
    pub(super) message: &'static str,
}

#[derive(Deserialize)]
pub(super) struct SaveConfigRequest {
    pub(super) raw_toml: String,
}

#[derive(Serialize)]
pub(super) struct OpsResponse {
    pub(super) qdrant_url: String,
    pub(super) tei_url: String,
    pub(super) collection: String,
    pub(super) mcp_http_url: String,
}

/// Per-invocation `Config` overrides accepted by `/v1/ask`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct AskRequestBody {
    pub(super) query: String,
    #[serde(default)]
    pub(super) collection: Option<String>,
    #[serde(default)]
    pub(super) since: Option<String>,
    #[serde(default)]
    pub(super) before: Option<String>,
    #[serde(default)]
    pub(super) diagnostics: Option<bool>,
    #[serde(default)]
    pub(super) explain: Option<bool>,
    /// Deprecated compatibility field. `false`/unset is accepted as a no-op;
    /// `true` is rejected before any ask execution.
    #[serde(default)]
    pub(super) graph: Option<bool>,
    #[serde(default)]
    pub(super) hybrid_search: Option<bool>,
    #[serde(default)]
    pub(super) ask_chunk_limit: Option<usize>,
    #[serde(default)]
    pub(super) ask_full_docs: Option<usize>,
    #[serde(default)]
    pub(super) ask_max_context_chars: Option<usize>,
    #[serde(default)]
    pub(super) ask_hybrid_candidates: Option<usize>,
    #[serde(default)]
    pub(super) ask_min_relevance_score: Option<f64>,
    #[serde(default)]
    pub(super) ask_doc_chunk_limit: Option<usize>,
    #[serde(default)]
    pub(super) ask_doc_fetch_concurrency: Option<usize>,
    #[serde(default)]
    pub(super) ask_backfill_chunks: Option<usize>,
    #[serde(default)]
    pub(super) ask_candidate_limit: Option<usize>,
    #[serde(default)]
    pub(super) ask_min_citations_nontrivial: Option<usize>,
    #[serde(default)]
    pub(super) ask_authoritative_domains: Option<Vec<String>>,
    #[serde(default)]
    pub(super) ask_authoritative_boost: Option<f64>,
}

#[derive(Serialize)]
pub(super) struct AskErrorBody {
    pub(super) kind: &'static str,
    pub(super) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) diagnostics: Option<serde_json::Value>,
}
