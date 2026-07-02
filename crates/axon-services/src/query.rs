use crate::events::{LogLevel, ServiceEvent, emit, synthesis_delta_handler_infallible};
use crate::types::{
    AskResult, DocumentBackend, EvaluateResult, Pagination, QueryHit, QueryResult, RetrieveResult,
    ServiceRetrieveVariantError, SuggestResult, Suggestion,
};
use axon_core::config::Config;
use axon_core::error::{ServiceError, diagnostics_from_error};
use axon_vector::ops::commands::ask::{ask_result, ask_result_with_deltas};
use axon_vector::ops::commands::discover_crawl_suggestions;
use axon_vector::ops::commands::evaluate_result;
use axon_vector::ops::commands::query_hits;
use axon_vector::ops::qdrant::DirectRetrieveResult;
use std::error::Error;
use tokio::sync::mpsc;

pub(crate) use self::code_search::default_code_search_refresh_backend;
pub use self::code_search::{
    CodeSearchProjectResult, CodeSearchRefreshBackend, CodeSearchRefreshResult, code_search,
    code_search_with_progress, refresh_code_search_index, refresh_code_search_index_with_backend,
    refresh_code_search_index_with_progress, resolve_code_search_project,
};
pub use self::retrieve::retrieve;

mod code_search;
mod retrieve;

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
    let results = query_hits(cfg, text, opts.limit.max(1), opts.offset)
        .await
        .map_err(|e| -> Box<dyn Error> {
            let message = format!(
                "vector query failed for {}: {e}",
                text.chars().take(80).collect::<String>()
            );
            wrap_service_error(message, e.as_ref())
        })?;
    Ok(QueryResult { results })
}
/// RAG ask: retrieve relevant context, then answer with LLM.
///
/// When `cfg.ask_stream` is true and `tx` is `Some`, synthesis tokens are
/// forwarded as `ServiceEvent::SynthesisDelta` events as they arrive.
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
    emit(
        &tx,
        ServiceEvent::Activity {
            kind: "thinking".to_string(),
            label: "Thinking".to_string(),
            detail: Some("Planning retrieval and answer synthesis".to_string()),
        },
    )
    .await;
    emit(
        &tx,
        ServiceEvent::Activity {
            kind: "tool".to_string(),
            label: "Retrieving context".to_string(),
            detail: Some(format!("Querying collection {}", cfg.collection)),
        },
    )
    .await;
    emit(
        &tx,
        ServiceEvent::Activity {
            kind: "tool".to_string(),
            label: "Synthesizing answer".to_string(),
            detail: Some("Streaming model response".to_string()),
        },
    )
    .await;
    let result = if cfg.ask_stream && tx.is_some() {
        ask_result_with_deltas(cfg, question, ask_delta_handler(tx.clone()))
            .await
            .map_err(|e| -> Box<dyn Error> {
                let message = format!(
                    "ask failed for {}: {e}",
                    question.chars().take(80).collect::<String>()
                );
                wrap_service_error(message, e.root_cause())
            })?
    } else {
        ask_result(cfg, question)
            .await
            .map_err(|e| -> Box<dyn Error> {
                let message = format!(
                    "ask failed for {}: {e}",
                    question.chars().take(80).collect::<String>()
                );
                wrap_service_error(message, e.as_ref())
            })?
    };
    if let Some(diagnostics) = &result.diagnostics {
        emit(
            &tx,
            ServiceEvent::Activity {
                kind: "done".to_string(),
                label: "Context selected".to_string(),
                detail: Some(format!(
                    "{} chunks, {} full docs",
                    diagnostics.chunks_selected, diagnostics.full_docs_selected
                )),
            },
        )
        .await;
    }
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ask complete".to_string(),
        },
    )
    .await;
    emit_ask_metrics(question, &result);
    Ok(result)
}

/// OPS-H2: emit ask-path timing + retrieval counts as both structured tracing
/// fields and Prometheus metrics. The `/metrics` endpoint (wired in
/// `src/web/server/routing.rs`) exposes these to Prometheus scrapers.
fn emit_ask_metrics(question: &str, result: &AskResult) {
    use metrics::{counter, histogram};
    let t = &result.timing_ms;
    let (candidate_pool, reranked_pool, chunks_selected, full_docs_selected, context_chars) =
        match &result.diagnostics {
            Some(d) => (
                d.candidate_pool,
                d.reranked_pool,
                d.chunks_selected,
                d.full_docs_selected,
                d.context_chars,
            ),
            None => (0, 0, 0, 0, 0),
        };
    tracing::info!(
        target: "axon::ask::metrics",
        query_preview = %question.chars().take(80).collect::<String>(),
        retrieval_ms = t.retrieval as u64,
        context_build_ms = t.context_build as u64,
        llm_ms = t.llm as u64,
        total_ms = t.total as u64,
        candidate_pool,
        reranked_pool,
        chunks_selected,
        full_docs_selected,
        context_chars,
        warnings = result.warnings.len(),
        "ask path completed"
    );
    // Prometheus metrics — no-ops when no recorder is installed (CLI/MCP-stdio).
    counter!("axon_ask_requests_total").increment(1);
    histogram!("axon_ask_retrieval_ms").record(t.retrieval as f64);
    histogram!("axon_ask_context_build_ms").record(t.context_build as f64);
    histogram!("axon_ask_llm_ms").record(t.llm as f64);
    histogram!("axon_ask_total_ms").record(t.total as f64);
    histogram!("axon_ask_candidate_pool").record(candidate_pool as f64);
    histogram!("axon_ask_chunks_selected").record(chunks_selected as f64);
    let warning_count = result.warnings.len() as u64;
    if warning_count > 0 {
        counter!("axon_ask_warnings_total").increment(warning_count);
    }
}

fn ask_delta_handler(tx: Option<mpsc::Sender<ServiceEvent>>) -> impl FnMut(&str) + Send {
    synthesis_delta_handler_infallible(tx, "ask")
}

/// RAG ask with token deltas emitted as the LLM streams.
#[must_use = "ask_stream returns a Result that should be handled"]
pub async fn ask_stream<F>(
    cfg: &Config,
    question: &str,
    on_delta: F,
) -> Result<String, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    let result = ask_result_with_deltas(cfg, question, on_delta)
        .await
        .map_err(|e| -> Box<dyn Error> {
            let message = format!(
                "ask failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            );
            wrap_service_error(message, e.root_cause())
        })?;
    Ok(result.answer)
}

/// RAG evaluate: run RAG and baseline answers, then judge with a second LLM call.
///
/// Returns the full structured evaluate payload without printing to stdout.
#[must_use = "evaluate returns a Result that should be handled"]
pub async fn evaluate(
    cfg: &Config,
    question: &str,
) -> Result<EvaluateResult, Box<dyn Error + Send + Sync>> {
    let mut derived = cfg.clone();
    derived.query = Some(question.to_string());
    derived.positional = Vec::new();
    evaluate_result(&derived)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            let message = format!(
                "evaluate failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            );
            Box::new(ServiceError::new(message))
        })
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
#[path = "query/payload_tests.rs"]
mod payload_tests;
#[cfg(test)]
#[path = "query/retrieve_tests.rs"]
mod retrieve_tests;
#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
