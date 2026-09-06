use super::super::error::HttpError;
use axon_core::config::Config;
use axon_services::client_contract::RestAskRequest as AskRequestBody;
use axon_services::context::ServiceContext;
use axon_services::query as query_svc;
use axon_services::transport::{AskTransportOverrides, apply_ask_overrides};
use axum::{Extension, Json, response::IntoResponse};
use lab_auth::AuthContext;
use std::sync::Arc;

use super::super::utils::auth_snapshot_from_auth;

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
    Extension(ctx): Extension<Arc<ServiceContext>>,
    auth: Option<Extension<AuthContext>>,
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
    if req.agent.is_some() && req.loadout.is_none() {
        return HttpError::bad_request("agent mode requires a revision-bound loadout")
            .into_response();
    }

    let resolved = match req.loadout.as_ref() {
        Some(binding) => match axon_services::loadout_context::resolve(&req_cfg, binding).await {
            Ok(value) => Some(value),
            Err(error) => {
                return HttpError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "loadout_resolution_failed",
                    error.to_string(),
                )
                .into_response();
            }
        },
        None => None,
    };
    let question = resolved.as_ref().map_or_else(
        || req.query.clone(),
        |context| format!("{}\n\n{}", context.prompt_context, req.query),
    );
    let mut result = match query_svc::ask_with_auth(
        &ctx,
        &req_cfg,
        &question,
        None,
        auth.as_ref()
            .map(|extension| auth_snapshot_from_auth(&extension.0)),
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            return HttpError::from_error_with_diagnostics(err.as_ref(), want_diagnostics)
                .into_response();
        }
    };
    result.query = req.query;
    if let Some(options) = req.agent {
        let binding = req.loadout.as_ref().expect("agent loadout validated");
        let resolution = resolved.as_ref().expect("agent loadout resolved");
        let prompt = format!(
            "{}\n\nUSER QUESTION:\n{}\n\nINITIAL RAG ANSWER:\n{}",
            resolution.prompt_context, result.query, result.answer
        );
        match axon_services::agent_runtime::run(
            &req_cfg,
            &binding.loadout_id,
            resolution.metadata.effective_revision,
            &prompt,
            options,
            axon_services::agent_runtime::AgentTurnOwner {
                principal: auth
                    .as_ref()
                    .map(|v| v.sub.clone())
                    .unwrap_or_else(|| "loopback-local".into()),
                profile_id: binding.integration_id.clone(),
            },
            axon_services::agent_runtime::configured_completion(req_cfg.clone()),
        )
        .await
        {
            Ok(agent) => {
                if let Some(answer) = agent.answer.clone() {
                    result.answer = answer;
                }
                result.agent = Some(agent);
            }
            Err(error) => {
                return HttpError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "agent_runtime_failed",
                    error.to_string(),
                )
                .into_response();
            }
        }
    }
    result.loadout = resolved.map(|value| value.metadata);
    Json(result).into_response()
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
