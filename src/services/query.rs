use crate::core::config::Config;
use crate::services::error::{ServiceError, diagnostics_from_error};
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{
    AskResult, EvaluateResult, Pagination, QueryHit, QueryResult, RetrieveOptions, RetrieveResult,
    ServiceRetrieveVariantError, SuggestResult, Suggestion,
};
use crate::vector::ops::commands::ask::ask_payload;
use crate::vector::ops::commands::discover_crawl_suggestions;
use crate::vector::ops::commands::evaluate_payload;
use crate::vector::ops::commands::query_results;
use crate::vector::ops::qdrant::{DirectRetrieveResult, retrieve_result};
use std::error::Error;
use tokio::sync::mpsc;

fn wrap_service_error(
    message: String,
    err: &(dyn Error + 'static),
) -> Box<dyn Error + Send + Sync + 'static> {
    if let Some(diagnostics) = diagnostics_from_error(err) {
        Box::new(ServiceError::with_diagnostics(message, diagnostics.clone()))
    } else {
        Box::new(ServiceError::new(message))
    }
}

// ── Pure mapping helpers (unit-testable, no live services required) ──────────

pub fn map_query_results(results: Vec<serde_json::Value>) -> Result<QueryResult, Box<dyn Error>> {
    let results = results
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            serde_json::from_value::<QueryHit>(value)
                .map_err(|e| -> Box<dyn Error> { format!("query result[{idx}]: {e}").into() })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(QueryResult { results })
}

pub fn map_retrieve_result(chunk_count: usize, content: String) -> RetrieveResult {
    RetrieveResult {
        chunk_count,
        content: if chunk_count == 0 {
            String::new()
        } else {
            content
        },
        requested_url: None,
        matched_url: None,
        truncated: false,
        warnings: Vec::new(),
        variant_errors: Vec::new(),
    }
}

pub fn map_direct_retrieve_result(result: DirectRetrieveResult) -> RetrieveResult {
    RetrieveResult {
        chunk_count: result.chunk_count,
        content: if result.chunk_count == 0 {
            String::new()
        } else {
            result.content
        },
        requested_url: Some(result.requested_url),
        matched_url: result.matched_url,
        truncated: result.truncated,
        warnings: result.warnings,
        variant_errors: result
            .variant_errors
            .into_iter()
            .map(|err| ServiceRetrieveVariantError {
                url: err.url,
                error: err.error,
            })
            .collect(),
    }
}

pub fn map_ask_payload(payload: serde_json::Value) -> Result<AskResult, Box<dyn Error>> {
    serde_json::from_value(payload).map_err(|e| format!("invalid ask payload: {e}").into())
}

pub fn map_evaluate_payload(payload: serde_json::Value) -> Result<EvaluateResult, Box<dyn Error>> {
    serde_json::from_value(payload).map_err(|e| format!("invalid evaluate payload: {e}").into())
}

pub fn map_suggest_payload(payload: &serde_json::Value) -> Result<SuggestResult, Box<dyn Error>> {
    let suggestions = payload
        .get("suggestions")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing suggestions array")?;
    let suggestions = suggestions
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let url = item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| -> Box<dyn Error> {
                    format!("suggestions[{i}]: missing url").into()
                })?;
            let reason = item
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Suggested by model")
                .to_string();
            Ok(Suggestion { url, reason })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(SuggestResult { suggestions })
}

// ── Service functions (call-through wrappers) ────────────────────────────────

/// Semantic vector search.
#[must_use = "query returns a Result that should be handled"]
pub async fn query(
    cfg: &Config,
    text: &str,
    opts: Pagination,
) -> Result<QueryResult, Box<dyn Error>> {
    let results = query_results(cfg, text, opts.limit.max(1), opts.offset)
        .await
        .map_err(|e| -> Box<dyn Error> {
            let message = format!(
                "vector query failed for {}: {e}",
                text.chars().take(80).collect::<String>()
            );
            wrap_service_error(message, e.as_ref())
        })?;
    map_query_results(results)
}

/// Retrieve stored document chunks for a URL.
#[must_use = "retrieve returns a Result that should be handled"]
pub async fn retrieve(
    cfg: &Config,
    url: &str,
    opts: RetrieveOptions,
) -> Result<RetrieveResult, Box<dyn Error>> {
    let result = retrieve_result(cfg, url, opts.max_points)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("retrieve failed for {url}: {e}").into() })?;
    Ok(map_direct_retrieve_result(result))
}

/// RAG ask: retrieve relevant context, then answer with LLM.
#[must_use = "ask returns a Result that should be handled"]
pub async fn ask(
    cfg: &Config,
    question: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<AskResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "starting ask: {}",
                question.chars().take(80).collect::<String>()
            ),
        },
    )
    .await;
    let payload = ask_payload(cfg, question)
        .await
        .map_err(|e| -> Box<dyn Error> {
            let message = format!(
                "ask failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            );
            wrap_service_error(message, e.as_ref())
        })?;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ask complete".to_string(),
        },
    )
    .await;
    map_ask_payload(payload)
}

/// RAG evaluate: run RAG and baseline answers, then judge with a second LLM call.
///
/// Returns the full structured evaluate payload without printing to stdout.
#[must_use = "evaluate returns a Result that should be handled"]
pub async fn evaluate(cfg: &Config, question: &str) -> Result<EvaluateResult, Box<dyn Error>> {
    let mut derived = cfg.clone();
    derived.query = Some(question.to_string());
    derived.positional = Vec::new();
    let payload = evaluate_payload(&derived)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!(
                "evaluate failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            )
            .into()
        })?;
    map_evaluate_payload(payload)
}

/// Suggest new URLs to crawl based on the current Qdrant index and an optional focus.
///
/// Returns accepted suggestions directly (no stdout side effects).
#[must_use = "suggest returns a Result that should be handled"]
pub async fn suggest(cfg: &Config, focus: Option<&str>) -> Result<SuggestResult, Box<dyn Error>> {
    let mut derived = cfg.clone();
    derived.query = focus.map(ToString::to_string);
    derived.positional = Vec::new();
    let desired = derived.search_limit.clamp(1, 100);
    let focus_str = focus.unwrap_or_default().to_string();
    let pairs: Vec<(String, String)> = discover_crawl_suggestions(&derived, &focus_str, desired)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("crawl suggestion discovery failed: {e}").into()
        })?;
    let suggestions = pairs
        .into_iter()
        .filter_map(|(url, reason)| {
            if !is_well_formed_suggest_url(&url) {
                tracing::warn!(
                    %url,
                    "suggest: dropped malformed suggestion URL"
                );
                return None;
            }
            Some(Suggestion { url, reason })
        })
        .collect();
    Ok(SuggestResult { suggestions })
}

/// Validate that a suggested URL is well-formed: parses as http/https with a
/// host that contains at least one dot (rules out single-label hosts like
/// `next.js` parsed as scheme=next, host=js).
///
/// This is intentionally stricter than `validate_url` (which only blocks SSRF
/// targets) — bare hostnames without TLD pass the SSRF guard but are useless
/// crawl seeds.
fn is_well_formed_suggest_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    // Bare IPs are fine; otherwise require at least one dot for a real TLD.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── map_retrieve_result ───────────────────────────────────────────────────

    #[test]
    fn map_retrieve_zero_chunks_returns_empty() {
        let result = map_retrieve_result(0, "some content".to_string());
        assert_eq!(result.chunk_count, 0);
        assert_eq!(result.content, "");
        assert_eq!(result.requested_url, None);
    }

    #[test]
    fn map_retrieve_nonzero_chunks() {
        let result = map_retrieve_result(5, "hello".to_string());
        assert_eq!(result.chunk_count, 5);
        assert_eq!(result.content, "hello");
    }

    #[test]
    fn map_retrieve_result_serializes_legacy_shape_when_metadata_absent() {
        let result = map_retrieve_result(5, "hello".to_string());
        let value = serde_json::to_value(result).expect("retrieve result serializes");
        assert_eq!(
            value,
            serde_json::json!({
                "chunk_count": 5,
                "content": "hello"
            })
        );
    }

    #[test]
    fn map_direct_retrieve_preserves_metadata() {
        let result = map_direct_retrieve_result(DirectRetrieveResult {
            requested_url: "example.com/docs".to_string(),
            matched_url: Some("https://example.com/docs".to_string()),
            chunk_count: 2,
            content: "hello".to_string(),
            truncated: true,
            warnings: vec!["partial result".to_string()],
            variant_errors: vec![crate::vector::ops::qdrant::RetrieveVariantError {
                url: "https://example.com/docs/".to_string(),
                error: "timeout".to_string(),
            }],
        });
        assert_eq!(result.requested_url.as_deref(), Some("example.com/docs"));
        assert_eq!(
            result.matched_url.as_deref(),
            Some("https://example.com/docs")
        );
        assert!(result.truncated);
        assert_eq!(result.warnings, vec!["partial result"]);
        assert_eq!(result.variant_errors[0].url, "https://example.com/docs/");
    }

    // ── map_suggest_payload ───────────────────────────────────────────────────

    #[test]
    fn map_suggest_valid() {
        let payload = json!({
            "suggestions": [
                { "url": "https://example.com/a", "reason": "A docs gap" },
                { "url": "https://example.com/b" }
            ]
        });
        let result = map_suggest_payload(&payload).unwrap();
        assert_eq!(result.suggestions.len(), 2);
        assert_eq!(result.suggestions[0].url, "https://example.com/a");
        assert_eq!(result.suggestions[0].reason, "A docs gap");
        assert_eq!(result.suggestions[1].url, "https://example.com/b");
        assert_eq!(result.suggestions[1].reason, "Suggested by model");
    }

    #[test]
    fn map_suggest_missing_suggestions() {
        let payload = json!({});
        let err = map_suggest_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("suggestions"),
            "error must mention 'suggestions', got: {err}"
        );
    }

    #[test]
    fn map_suggest_entry_missing_url() {
        let payload = json!({
            "suggestions": [{ "reason": "no url key here" }]
        });
        let err = map_suggest_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("suggestions[0]"),
            "error must reference suggestions[0], got: {msg}"
        );
    }

    #[test]
    fn map_suggest_empty_suggestions() {
        let payload = json!({ "suggestions": [] });
        let result = map_suggest_payload(&payload).unwrap();
        assert!(result.suggestions.is_empty());
    }

    // ── map_ask_payload ──────────────────────────────────────────────────────

    #[test]
    fn map_ask_payload_typed() {
        let payload = json!({
            "query": "what is axon?",
            "answer": "A crawler.",
            "diagnostics": null,
            "timing_ms": {
                "retrieval": 1,
                "context_build": 2,
                "graph": 0,
                "llm": 3,
                "total": 6
            }
        });
        let result = map_ask_payload(payload).unwrap();
        assert_eq!(result.query, "what is axon?");
        assert_eq!(result.answer, "A crawler.");
        assert!(result.diagnostics.is_none());
        assert_eq!(result.timing_ms.total, 6);
    }

    #[test]
    fn map_ask_payload_preserves_adaptive_diagnostics() {
        let payload = json!({
            "query": "what is axon?",
            "answer": "A crawler.",
            "diagnostics": {
                "candidate_pool": 12,
                "reranked_pool": 8,
                "chunks_selected": 4,
                "full_docs_selected": 2,
                "supplemental_selected": 1,
                "context_chars": 3000,
                "graph_entities": 0,
                "graph_context_chars": 0,
                "full_doc_fetch_skipped": true,
                "full_doc_fetch_skip_reason": "low_complexity",
                "detected_complexity": "simple",
                "resolved_full_docs": 2,
                "full_docs_source": "adaptive",
                "min_relevance_score": 0.4,
                "doc_fetch_concurrency": 8,
                "top_domains": ["docs.example.com"],
                "authority_ratio": 0.75
            },
            "timing_ms": {
                "retrieval": 1,
                "context_build": 2,
                "graph": 0,
                "llm": 3,
                "total": 6
            }
        });
        let result = map_ask_payload(payload).unwrap();
        let diagnostics = result.diagnostics.expect("diagnostics should deserialize");
        assert!(diagnostics.full_doc_fetch_skipped);
        assert_eq!(diagnostics.full_doc_fetch_skip_reason, "low_complexity");
        assert_eq!(diagnostics.detected_complexity, "simple");
        assert_eq!(diagnostics.resolved_full_docs, 2);
        assert_eq!(diagnostics.full_docs_source, "adaptive");
    }

    #[test]
    fn map_ask_payload_rejects_invalid_shape() {
        let err = map_ask_payload(json!({ "answer": "missing query and timing" })).unwrap_err();
        assert!(err.to_string().contains("invalid ask payload"));
    }

    // ── map_evaluate_payload ─────────────────────────────────────────────────

    #[test]
    fn map_evaluate_payload_typed() {
        let payload = json!({
            "query": "what is axon?",
            "rag_answer": "RAG",
            "baseline_answer": "Baseline",
            "analysis_answer": "Analysis",
            "source_urls": ["https://example.com/a"],
            "crawl_suggestions": [{ "url": "https://example.com/b", "reason": "gap" }],
            "crawl_enqueue_outcomes": [],
            "ref_chunk_count": 3,
            "diagnostics": null,
            "timing_ms": {
                "retrieval": 1,
                "context_build": 2,
                "rag_llm": 3,
                "baseline_llm": 4,
                "research_elapsed_ms": 5,
                "analysis_llm_ms": 6,
                "total": 21
            }
        });
        let result = map_evaluate_payload(payload).unwrap();
        assert_eq!(result.query, "what is axon?");
        assert_eq!(result.source_urls, vec!["https://example.com/a"]);
        assert_eq!(result.crawl_suggestions[0].reason, "gap");
        assert_eq!(result.timing_ms.total, 21);
    }

    #[test]
    fn map_evaluate_payload_rejects_invalid_shape() {
        let err = map_evaluate_payload(json!({ "query": "missing fields" })).unwrap_err();
        assert!(err.to_string().contains("invalid evaluate payload"));
    }

    // ── map_query_results ─────────────────────────────────────────────────────

    #[test]
    fn map_query_results_passthrough_empty() {
        let result = map_query_results(vec![]).unwrap();
        assert!(result.results.is_empty());
    }

    #[test]
    fn map_query_results_typed_nonempty() {
        let items = vec![
            json!({
                "rank": 1,
                "score": 0.9,
                "rerank_score": 1.1,
                "url": "https://a.com",
                "source": "a.com",
                "snippet": "alpha",
                "chunk_index": 2
            }),
            json!({
                "rank": 2,
                "score": 0.8,
                "rerank_score": 0.95,
                "url": "https://b.com",
                "source": "b.com",
                "snippet": "bravo",
                "chunk_index": null
            }),
        ];
        let result = map_query_results(items).unwrap();
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0].url, "https://a.com");
        assert_eq!(result.results[0].chunk_index, Some(2));
        assert_eq!(result.results[1].source, "b.com");
        assert_eq!(result.results[1].chunk_index, None);
    }

    #[test]
    fn map_query_results_rejects_missing_required_fields() {
        let err = map_query_results(vec![json!({ "url": "https://a.com" })]).unwrap_err();
        assert!(
            err.to_string().contains("query result[0]"),
            "error should identify the bad result index, got: {err}"
        );
    }

    #[tokio::test]
    async fn query_reports_typed_diagnostics_payload_without_ask_diagnostics() {
        use httpmock::Method::POST;
        use httpmock::MockServer;

        // TEI succeeds so query proceeds to vector mode probe.
        let tei = MockServer::start_async().await;
        tei.mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
        })
        .await;

        // Qdrant probe fails with 404, which should surface as structured diagnostics.
        let qdrant = MockServer::start_async().await;
        qdrant
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path_matches(regex::Regex::new("/collections/").unwrap());
                then.status(404);
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.tei_url = tei.base_url();
        cfg.qdrant_url = qdrant.base_url();
        cfg.collection = "diag_test_collection".to_string();
        cfg.ask_diagnostics = false;

        let err = query(
            &cfg,
            "diagnostics regression test query",
            Pagination {
                limit: 5,
                offset: 0,
            },
        )
        .await
        .expect_err("query should fail when collection is missing");

        let diag = diagnostics_from_error(err.as_ref())
            .expect("diagnostics payload should be attached without ask_diagnostics");
        assert_eq!(diag["stage"], "query_vector_search_dispatch");
        assert_eq!(diag["collection"], "diag_test_collection");
        assert_eq!(
            diag["qdrant_url"],
            reqwest::Url::parse(&qdrant.base_url()).unwrap().to_string()
        );
        assert_eq!(diag["query_len"], "diagnostics regression test query".len());
        assert_eq!(diag["mode"]["hybrid_search_enabled"], true);
        assert_eq!(diag["search_context"]["command"], "query");
        assert_eq!(diag["search_context"]["request_limit"], 80);
        assert_eq!(diag["search_context"]["sparse_query_empty"], false);
        assert!(diag["error"].as_str().unwrap_or("").contains("404"));
    }
}
