use super::super::types::{AskErrorBody, AskRequestBody};
use crate::core::config::Config;
use crate::services::error::diagnostics_from_error;
use crate::services::query as query_svc;
use axum::{Extension, Json, http::StatusCode, response::IntoResponse};
use std::error::Error;
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

/// Map a service error chain to (status, kind) using simple message-based
/// heuristics over the chain. Falls back to 500/internal.
pub(crate) fn classify_ask_error(err: &(dyn Error + 'static)) -> (StatusCode, &'static str) {
    let mut buf = String::new();
    let mut cur: Option<&(dyn Error + 'static)> = Some(err);
    while let Some(e) = cur {
        buf.push_str(&e.to_string());
        buf.push('\n');
        cur = e.source();
    }
    let lc = buf.to_lowercase();
    if lc.contains("query is required")
        || lc.contains("invalid collection")
        || lc.contains("invalid query")
        || lc.contains("missing required")
    {
        return (StatusCode::BAD_REQUEST, "bad_request");
    }
    if lc.contains("qdrant")
        || lc.contains("tei")
        || lc.contains("connection refused")
        || lc.contains("upstream")
        || lc.contains("timed out")
        || lc.contains("timeout")
        || lc.contains("dns")
        || lc.contains("502")
        || lc.contains("503")
    {
        return (StatusCode::BAD_GATEWAY, "upstream");
    }
    (StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

pub async fn v1_ask(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if req.graph.unwrap_or(false) {
        return (
            StatusCode::BAD_REQUEST,
            Json(AskErrorBody {
                kind: "bad_request",
                message: "graph retrieval is not supported; omit graph or set graph to false"
                    .to_string(),
                diagnostics: None,
            }),
        )
            .into_response();
    }
    if req.query.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AskErrorBody {
                kind: "bad_request",
                message: "query is required".to_string(),
                diagnostics: None,
            }),
        )
            .into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(AskErrorBody {
                kind: "payload_too_large",
                message: format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"),
                diagnostics: None,
            }),
        )
            .into_response();
    }

    let mut req_cfg = (*cfg).clone();
    apply_ask_overrides(&mut req_cfg, &req);
    let want_diagnostics = req_cfg.ask_diagnostics;

    match query_svc::ask(&req_cfg, &req.query, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => {
            let (status, kind) = classify_ask_error(err.as_ref());
            let diagnostics = if want_diagnostics {
                diagnostics_from_error(err.as_ref()).cloned()
            } else {
                None
            };
            (
                status,
                Json(AskErrorBody {
                    kind,
                    message: err.to_string(),
                    diagnostics,
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_ask_overrides_clamps_request_tuning() {
        let mut cfg = Config::default();
        let req = AskRequestBody {
            query: "test".to_string(),
            ask_chunk_limit: Some(1),
            ask_full_docs: Some(999),
            ask_max_context_chars: Some(1),
            ask_hybrid_candidates: Some(999),
            ask_doc_chunk_limit: Some(1),
            ask_doc_fetch_concurrency: Some(0),
            ask_backfill_chunks: Some(999),
            ask_candidate_limit: Some(1),
            ask_min_citations_nontrivial: Some(99),
            ask_authoritative_boost: Some(10.0),
            ..AskRequestBody::default()
        };

        apply_ask_overrides(&mut cfg, &req);

        assert_eq!(cfg.ask_chunk_limit, 3);
        assert_eq!(cfg.ask_full_docs, 20);
        assert!(cfg.ask_full_docs_explicit);
        assert_eq!(cfg.ask_max_context_chars, 20_000);
        assert_eq!(cfg.ask_hybrid_candidates, 500);
        assert_eq!(cfg.ask_doc_chunk_limit, 8);
        assert_eq!(cfg.ask_doc_fetch_concurrency, 1);
        assert_eq!(cfg.ask_backfill_chunks, 20);
        assert_eq!(cfg.ask_candidate_limit, 8);
        assert_eq!(cfg.ask_min_citations_nontrivial, 5);
        assert!((cfg.ask_authoritative_boost - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_ask_overrides_marks_full_docs_as_explicit() {
        let mut cfg = Config::default();
        let req = AskRequestBody {
            query: "test".to_string(),
            ask_full_docs: Some(2),
            ..AskRequestBody::default()
        };

        apply_ask_overrides(&mut cfg, &req);

        assert_eq!(cfg.ask_full_docs, 2);
        assert!(cfg.ask_full_docs_explicit);
    }
}
