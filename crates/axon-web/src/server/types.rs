use serde::{Deserialize, Serialize};

/// Hard limit on `/v1/ask` request bodies. Matches the existing 64 KiB cap used
/// by `dispatch_vector_search` so the web surface mirrors MCP behavior.
pub(super) const ASK_BODY_LIMIT: usize = 64 * 1024;
/// Reject ask queries longer than this (defense-in-depth above body cap).
pub(super) const ASK_QUERY_MAX_CHARS: usize = 16 * 1024;

/// Hard limit on `/v1/memories/import` and `/v1/memories/export` request
/// bodies. A prior security review flagged the originating draft of these
/// routes for shipping with no size control at all; 10 MiB is a conservative
/// bound generous enough for a real bulk-import bundle without opening an
/// unbounded-body DoS vector, matching the conservative default the plan
/// calls for (`docs/pipeline-unification/plans/
/// 2026-07-08-rest-memory-surface.md` Task 3).
pub(super) const MEMORY_IMPORT_EXPORT_BODY_LIMIT: usize = 10 * 1024 * 1024;

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
pub(super) struct EnvConfigResponse {
    pub(super) path: String,
    pub(super) raw_env: String,
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

#[derive(Deserialize)]
pub(super) struct SaveEnvConfigRequest {
    pub(super) raw_env: String,
}

#[derive(Deserialize)]
pub(super) struct PanelCommandRequest {
    pub(super) command: String,
}

#[derive(Serialize)]
pub(super) struct PanelCommandResponse {
    pub(super) command: String,
    pub(super) action: serde_json::Value,
    pub(super) result: serde_json::Value,
}

#[derive(Serialize)]
pub(super) struct PanelStatusResponse {
    pub(super) payload: serde_json::Value,
    pub(super) text: String,
    pub(super) totals: axon_services::types::StatusTotals,
}

#[derive(Serialize)]
pub(super) struct PanelDoctorResponse {
    pub(super) payload: serde_json::Value,
}

#[derive(Serialize)]
pub(super) struct OpsResponse {
    pub(super) qdrant_url: String,
    pub(super) tei_url: String,
    pub(super) collection: String,
    pub(super) mcp_http_url: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PanelCollectionsResponse {
    pub(super) collections: Vec<String>,
}
