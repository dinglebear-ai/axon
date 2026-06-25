use super::super::error::HttpError;
use axon_core::config::Config;
use axon_services::client_contract::RestAskRequest as AskRequestBody;
use axon_services::query as query_svc;
use axon_services::transport::{AskTransportOverrides, apply_ask_overrides};
use axum::{Extension, Json, response::IntoResponse};
use std::sync::Arc;

pub(super) fn ask_transport_overrides(req: &AskRequestBody) -> AskTransportOverrides {
    AskTransportOverrides {
        collection: req.collection.clone(),
        since: req.since.clone(),
        before: req.before.clone(),
        diagnostics: req.diagnostics,
        explain: req.explain,
        hybrid_search: req.hybrid_search,
        ask_chunk_limit: req.ask_chunk_limit,
        ask_full_docs: req.ask_full_docs,
        ask_max_context_chars: req.ask_max_context_chars,
        ask_hybrid_candidates: req.ask_hybrid_candidates,
        ask_min_relevance_score: req.ask_min_relevance_score,
        ask_doc_chunk_limit: req.ask_doc_chunk_limit,
        ask_doc_fetch_concurrency: req.ask_doc_fetch_concurrency,
        ask_backfill_chunks: req.ask_backfill_chunks,
        ask_candidate_limit: req.ask_candidate_limit,
        ask_min_citations_nontrivial: req.ask_min_citations_nontrivial,
        ask_authoritative_domains: req.ask_authoritative_domains.clone(),
        ask_authoritative_boost: req.ask_authoritative_boost,
    }
}

#[utoipa::path(
    post,
    path = "/v1/ask",
    request_body = AskRequestBody,
    responses(
        (status = 200, description = "RAG answer", body = serde_json::Value),
        (status = 400, description = "Invalid ask request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Ask request exceeds limits", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or LLM service unavailable", body = crate::server::error::ErrorBody),
        (status = 504, description = "Upstream request timed out", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_ask(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if req.query.trim().is_empty() {
        return HttpError::bad_request("query is required").into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return HttpError::payload_too_large(format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"))
            .into_response();
    }

    let req_cfg = apply_ask_overrides(&cfg, ask_transport_overrides(&req));

    // SEC-M1: validate the collection override at the handler boundary, before it
    // flows into retrieval — defense-in-depth matching the MCP path (which validates
    // early via the same shared helper at src/mcp/server/common.rs). Reuses the one
    // source-of-truth validator from `core::config`; no duplicated regex. The
    // downstream `qdrant_collection_endpoint` choke point still validates, so this is
    // belt-and-suspenders rather than the sole guard.
    if let Err(reason) = axon_core::config::validate_collection_name(&req_cfg.collection) {
        return HttpError::bad_request(format!("invalid collection name: {reason}"))
            .into_response();
    }

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
