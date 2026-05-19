use super::super::error::HttpError;
use super::super::types::AskRequestBody;
use crate::core::config::Config;
use crate::services::query as query_svc;
use axum::{Extension, Json, response::IntoResponse};
use std::sync::Arc;

/// Apply the per-request `ask_*` overrides from the body to a cloned `Config`.
///
/// Keep these bounds in sync with `src/core/config/parse/tuning.rs` so
/// in-process CLI and `/v1/ask` callers get the same retrieval behavior.
fn apply_ask_overrides(req_cfg: &mut Config, req: &AskRequestBody) {
    if let Some(c) = req.collection.as_ref() {
        req_cfg.collection = c.clone();
    }
    if let Some(s) = req.since.as_ref() {
        req_cfg.since = Some(s.clone());
    }
    if let Some(b) = req.before.as_ref() {
        req_cfg.before = Some(b.clone());
    }
    if let Some(d) = req.diagnostics {
        req_cfg.ask_diagnostics = d;
    }
    if let Some(explain) = req.explain {
        req_cfg.ask_explain = explain;
        if explain {
            req_cfg.ask_diagnostics = true;
        }
    }
    if let Some(h) = req.hybrid_search {
        req_cfg.hybrid_search_enabled = h;
    }
    if let Some(v) = req.ask_chunk_limit {
        req_cfg.ask_chunk_limit = v.clamp(3, 40);
    }
    if let Some(v) = req.ask_full_docs {
        req_cfg.ask_full_docs = v.clamp(1, 20);
        req_cfg.ask_full_docs_explicit = true;
    }
    if let Some(v) = req.ask_max_context_chars {
        req_cfg.ask_max_context_chars = v.clamp(20_000, 1_000_000);
    }
    if let Some(v) = req.ask_hybrid_candidates {
        req_cfg.ask_hybrid_candidates = v.clamp(10, 500);
    }
    if let Some(v) = req.ask_min_relevance_score {
        req_cfg.ask_min_relevance_score = v.clamp(-1.0, 2.0);
    }
    if let Some(v) = req.ask_doc_chunk_limit {
        req_cfg.ask_doc_chunk_limit = v.clamp(8, 2000);
    }
    if let Some(v) = req.ask_doc_fetch_concurrency {
        req_cfg.ask_doc_fetch_concurrency = v.clamp(1, 16);
    }
    if let Some(v) = req.ask_backfill_chunks {
        req_cfg.ask_backfill_chunks = v.clamp(0, 20);
    }
    if let Some(v) = req.ask_candidate_limit {
        req_cfg.ask_candidate_limit = v.clamp(8, 300);
    }
    if let Some(v) = req.ask_min_citations_nontrivial {
        req_cfg.ask_min_citations_nontrivial = v.clamp(1, 5);
    }
    if let Some(v) = req.ask_authoritative_domains.as_ref() {
        req_cfg.ask_authoritative_domains = v.clone();
    }
    if let Some(v) = req.ask_authoritative_boost {
        req_cfg.ask_authoritative_boost = v.clamp(0.0, 0.5);
    }
}

#[utoipa::path(
    post,
    path = "/v1/ask",
    request_body = AskRequestBody,
    responses(
        (status = 200, description = "RAG answer", body = serde_json::Value),
        (status = 400, description = "Invalid ask request", body = crate::web::server::error::ErrorBody),
        (status = 413, description = "Ask request exceeds limits", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or LLM service unavailable", body = crate::web::server::error::ErrorBody),
        (status = 504, description = "Upstream request timed out", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_ask(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if req.graph.unwrap_or(false) {
        return HttpError::bad_request(
            "graph retrieval is not supported; omit graph or set graph to false",
        )
        .into_response();
    }
    if req.query.trim().is_empty() {
        return HttpError::bad_request("query is required").into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return HttpError::payload_too_large(format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"))
            .into_response();
    }

    let mut req_cfg = (*cfg).clone();
    apply_ask_overrides(&mut req_cfg, &req);
    let want_diagnostics = req_cfg.ask_diagnostics;

    match query_svc::ask(&req_cfg, &req.query, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => {
            HttpError::from_error_with_diagnostics(err.as_ref(), want_diagnostics).into_response()
        }
    }
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
