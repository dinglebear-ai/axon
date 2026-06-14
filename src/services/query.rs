use crate::core::config::{Config, ConfigOverrides, ScrapeFormat};
use crate::core::error::{ServiceError, diagnostics_from_error};
use crate::services::document::{
    decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
};
use crate::services::events::{LogLevel, ServiceEvent, emit, synthesis_delta_handler_infallible};
use crate::services::scrape as scrape_svc;
use crate::services::types::{
    AskResult, DocumentBackend, EvaluateResult, Pagination, QueryHit, QueryResult, RetrieveOptions,
    RetrieveResult, ServiceRetrieveVariantError, SuggestResult, Suggestion,
};
use crate::vector::ops::commands::ask::{ask_result, ask_result_with_deltas};
use crate::vector::ops::commands::discover_crawl_suggestions;
use crate::vector::ops::commands::evaluate_result;
use crate::vector::ops::commands::query_hits;
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

/// OPS-H2 (bounded): emit ask-path timing + retrieval counts as STRUCTURED
/// tracing fields at the service boundary so an operator can scrape ask latency
/// and pool sizes from logs (JSON file sink) without a metrics backend.
///
/// This is the minimal, reviewable observability step. A real `/metrics`
/// (Prometheus) surface is intentionally out of scope for this pass.
/// TODO(OPS-H2): expose /metrics (Prometheus) at the web server bootstrap
/// (`src/web/server/`) and convert these fields into counters/histograms —
/// see .full-review/04-best-practices.md (OPS-H2).
fn emit_ask_metrics(question: &str, result: &AskResult) {
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
            format!(
                "evaluate failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            )
            .into()
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
#[path = "query_tests.rs"]
mod tests;
