use crate::core::config::{Config, ConfigOverrides, ScrapeFormat};
use crate::services::document::{
    decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
};
use crate::services::error::{ServiceError, diagnostics_from_error};
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::scrape as scrape_svc;
use crate::services::types::{
    AskResult, DocumentBackend, EvaluateResult, Pagination, QueryHit, QueryResult, RetrieveOptions,
    RetrieveResult, ServiceRetrieveVariantError, SuggestResult, Suggestion,
};
use crate::vector::ops::commands::ask::ask_payload;
use crate::vector::ops::commands::discover_crawl_suggestions;
use crate::vector::ops::commands::evaluate_payload;
use crate::vector::ops::commands::query_results;
use crate::vector::ops::qdrant::{DirectRetrieveResult, retrieve_result};
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;

const RETRIEVE_STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

struct ResolvedDocument {
    backend: DocumentBackend,
    content: String,
    chunk_count: usize,
    matched_url: Option<String>,
    warnings: Vec<String>,
    variant_errors: Vec<ServiceRetrieveVariantError>,
    source_truncated: bool,
    refresh_status: Option<String>,
}

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
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: None,
        refresh_status: None,
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
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: Some(DocumentBackend::Qdrant),
        refresh_status: None,
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
) -> Result<RetrieveResult, Box<dyn Error + Send + Sync>> {
    let pinned_backend = decode_document_cursor_backend(opts.cursor.as_deref()).map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("invalid retrieve cursor for {url}: {e}").into()
        },
    )?;
    let resolved = resolve_document(cfg, url, opts.max_points, pinned_backend).await?;
    let page = paginate_document(
        &resolved.content,
        opts.cursor.as_deref(),
        opts.token_budget,
        resolved.backend,
    )
    .map_err(|e| -> Box<dyn Error + Send + Sync> {
        format!("paginate retrieve result for {url}: {e}").into()
    })?;
    Ok(RetrieveResult {
        chunk_count: resolved.chunk_count,
        content: page.content,
        requested_url: Some(url.to_string()),
        matched_url: resolved.matched_url,
        truncated: page.truncated || resolved.source_truncated,
        warnings: resolved.warnings,
        variant_errors: resolved.variant_errors,
        token_estimate: page.token_estimate,
        next_cursor: page.next_cursor,
        remaining_tokens_estimate: page.remaining_tokens_estimate,
        backend: Some(page.backend),
        refresh_status: resolved.refresh_status,
    })
}

async fn resolve_document(
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
    pinned_backend: Option<DocumentBackend>,
) -> Result<ResolvedDocument, Box<dyn Error + Send + Sync>> {
    if let Some(backend) = pinned_backend {
        return match backend {
            DocumentBackend::Qdrant => resolve_qdrant_document(cfg, url, max_points)
                .await?
                .ok_or_else(|| {
                    "retrieve cursor requires qdrant backend but no stored chunks exist"
                        .to_string()
                        .into()
                }),
            DocumentBackend::StoredSource => resolve_stored_source_document(cfg, url)
                .await?
                .ok_or_else(|| {
                    "retrieve cursor requires stored_source backend but no source file exists"
                        .to_string()
                        .into()
                }),
            DocumentBackend::LiveScrape => resolve_live_scrape_document(cfg, url, "cursor").await,
        };
    }

    let mut qdrant_error: Option<String> = None;
    match resolve_qdrant_document(cfg, url, max_points).await {
        Ok(Some(qdrant)) => return Ok(qdrant),
        Ok(None) => {}
        Err(err) => qdrant_error = Some(err.to_string()),
    }

    if let Some(stored) = resolve_stored_source_document(cfg, url).await? {
        if stored.refresh_status.as_deref() == Some("stale") {
            match resolve_live_scrape_document(cfg, url, "stale").await {
                Ok(mut refreshed) => {
                    refreshed.warnings.extend(stored.warnings);
                    if let Some(err) = qdrant_error {
                        refreshed
                            .warnings
                            .push(format!("qdrant backend unavailable during retrieve: {err}"));
                    }
                    return Ok(refreshed);
                }
                Err(err) => {
                    let mut stale = stored;
                    stale.warnings.push(format!(
                        "live scrape refresh failed; falling back to stale stored source: {err}"
                    ));
                    if let Some(qdrant_err) = qdrant_error {
                        stale.warnings.push(format!(
                            "qdrant backend unavailable during retrieve: {qdrant_err}"
                        ));
                    }
                    return Ok(stale);
                }
            }
        }
        let mut stored = stored;
        if let Some(err) = qdrant_error {
            stored
                .warnings
                .push(format!("qdrant backend unavailable during retrieve: {err}"));
        }
        return Ok(stored);
    }

    let mut live = resolve_live_scrape_document(cfg, url, "miss").await?;
    if let Some(err) = qdrant_error {
        live.warnings
            .push(format!("qdrant backend unavailable during retrieve: {err}"));
    }
    Ok(live)
}

async fn resolve_qdrant_document(
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
) -> Result<Option<ResolvedDocument>, Box<dyn Error + Send + Sync>> {
    let result = retrieve_result(cfg, url, max_points).await.map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("qdrant retrieve failed for {url}: {e}").into()
        },
    )?;
    if result.chunk_count == 0 {
        return Ok(None);
    }
    let mapped = map_direct_retrieve_result(result);
    Ok(Some(ResolvedDocument {
        backend: DocumentBackend::Qdrant,
        content: mapped.content,
        chunk_count: mapped.chunk_count,
        matched_url: mapped.matched_url,
        warnings: mapped.warnings,
        variant_errors: mapped.variant_errors,
        source_truncated: mapped.truncated,
        refresh_status: None,
    }))
}

async fn resolve_stored_source_document(
    cfg: &Config,
    url: &str,
) -> Result<Option<ResolvedDocument>, Box<dyn Error + Send + Sync>> {
    let Some(stored) = read_latest_stored_source(&cfg.output_dir, url)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("stored source lookup failed for {url}: {e}").into()
        })?
    else {
        return Ok(None);
    };
    let stale = is_stale(stored.modified_at, RETRIEVE_STALE_AFTER);
    let mut warnings = Vec::new();
    if stale {
        warnings.push(format!(
            "stored source is stale (> {} hours old); attempting live refresh",
            RETRIEVE_STALE_AFTER.as_secs() / 3600
        ));
    }
    warnings.push(format!(
        "using stored source file {}",
        stored.path.display()
    ));
    Ok(Some(ResolvedDocument {
        backend: DocumentBackend::StoredSource,
        content: stored.content,
        chunk_count: 0,
        matched_url: Some(url.to_string()),
        warnings,
        variant_errors: Vec::new(),
        source_truncated: false,
        refresh_status: stale.then(|| "stale".to_string()),
    }))
}

async fn resolve_live_scrape_document(
    cfg: &Config,
    url: &str,
    reason: &str,
) -> Result<ResolvedDocument, Box<dyn Error + Send + Sync>> {
    let scrape_cfg = cfg.apply_overrides(&ConfigOverrides {
        format: Some(ScrapeFormat::Markdown),
        output_path: Some(None),
        ..ConfigOverrides::default()
    });
    let result = scrape_svc::scrape(&scrape_cfg, url, None).await.map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("live scrape refresh failed for {url}: {e}").into()
        },
    )?;
    let refresh_status = match reason {
        "stale" => Some("refreshed_stale".to_string()),
        "miss" => Some("refreshed_missing".to_string()),
        "cursor" => Some("cursor_live_scrape".to_string()),
        _ => Some(reason.to_string()),
    };
    let warning = match reason {
        "stale" => "served fresh live scrape because stored source was stale",
        "miss" => "served fresh live scrape because no indexed or stored content was available",
        "cursor" => "continued retrieve via live scrape backend",
        _ => "served fresh live scrape content",
    };
    Ok(ResolvedDocument {
        backend: DocumentBackend::LiveScrape,
        content: result.output,
        chunk_count: 0,
        matched_url: Some(result.url),
        warnings: vec![warning.to_string()],
        variant_errors: Vec::new(),
        source_truncated: false,
        refresh_status,
    })
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
        assert_eq!(result.backend, None);
    }

    #[test]
    fn map_retrieve_nonzero_chunks() {
        let result = map_retrieve_result(5, "hello".to_string());
        assert_eq!(result.chunk_count, 5);
        assert_eq!(result.content, "hello");
        assert_eq!(result.next_cursor, None);
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
        assert_eq!(result.backend, Some(DocumentBackend::Qdrant));
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
