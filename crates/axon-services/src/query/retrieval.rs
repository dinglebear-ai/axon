//! `query` semantic search routed through the new `axon-retrieval` engine.
//!
//! This is the cutover seam for issue #298: plain `query` (semantic search, no
//! LLM) now embeds + hybrid-searches through
//! [`axon_retrieval::run_query`] instead of legacy axon-vector's
//! `query_hits`. `ask`/`evaluate`/`retrieve` also now route their SEARCH +
//! CONTEXT half through this same engine — see `ask_retrieval.rs`.

use std::error::Error;
use std::sync::Arc;

use axon_api::result::QueryHit;
use axon_core::config::Config;
use axon_core::error::ServiceError;
use axon_core::logging::log_info;
use axon_embedding::provider::EmbeddingProvider;
use axon_retrieval::{QueryServiceRequest, run_query};
use axon_vectors::store::VectorStore;

use crate::context::{ServiceContext, build_read_stores_from_config};
use crate::types::{Pagination, QueryResult};

/// Run semantic `query` through the retrieval engine and map hits to
/// [`QueryResult`], using `ctx.cfg()` for collection + endpoints.
pub async fn query_via_retrieval(
    ctx: &ServiceContext,
    text: &str,
    opts: Pagination,
) -> Result<QueryResult, Box<dyn Error>> {
    query_via_retrieval_with_cfg(ctx, ctx.cfg(), text, opts).await
}

/// Run semantic `query` through the retrieval engine with an explicit effective
/// `cfg` (honors per-request overrides such as the collection) while resolving
/// read-plane stores from `ctx`.
///
/// Prefers the context's target local-source runtime stores when present
/// (`serve`/`mcp`/`--wait`); otherwise builds read-plane stores from `cfg`
/// (enqueue-only CLI). Never falls back to the legacy vector path.
pub async fn query_via_retrieval_with_cfg(
    ctx: &ServiceContext,
    cfg: &Config,
    text: &str,
    opts: Pagination,
) -> Result<QueryResult, Box<dyn Error>> {
    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(
            "query requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
                .to_string(),
        )));
    }

    let (store, provider, provider_id, model, dimensions) = resolve_stores(ctx, cfg).await;

    log_info("retrieval: axon-retrieval engine");

    let limit = opts.limit.max(1) as u32;
    // The engine returns per-chunk matches without pagination; fetch enough to
    // cover the requested offset+limit window, then apply the window here.
    let fetch_limit = (opts.offset as u32).saturating_add(limit);
    let (since, before) = normalize_time_bounds(cfg, chrono::Utc::now())?;
    let result = run_query(
        store,
        provider,
        provider_id,
        model,
        dimensions,
        QueryServiceRequest {
            query: text.to_string(),
            collection: cfg.collection.clone(),
            limit: fetch_limit.max(1),
            hybrid: cfg.hybrid_search_enabled,
            since,
            before,
        },
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        // Diagnostics wiring lost in the #298 cutover from the legacy
        // `axon_vector` dispatch path (which built this via
        // `ServiceError::vector_dispatch_failure`) -- restore it here so
        // `--diagnostics` still surfaces a structured payload on failure.
        Box::new(ServiceError::vector_dispatch_failure_with_message(
            format!(
                "retrieval query failed for {}: {e}",
                text.chars().take(80).collect::<String>()
            ),
            "query_vector_search_dispatch",
            cfg,
            text.len(),
            serde_json::json!({ "limit": fetch_limit }),
            &e,
        ))
    })?;

    let results = result
        .hits
        .into_iter()
        .skip(opts.offset)
        .take(opts.limit.clamp(1, axon_api::MAX_CANONICAL_CITATIONS))
        .enumerate()
        .map(|(idx, hit)| QueryHit {
            rank: (opts.offset + idx + 1) as u64,
            score: hit.score,
            rerank_score: hit.score,
            source: display_source(&hit.canonical_uri),
            url: hit.canonical_uri,
            snippet: hit.text,
            citation: hit.citation,
            chunk_index: None,
            file_path: None,
            symbol: None,
            kind: None,
            start_line: None,
            end_line: None,
            file_type: None,
            language: None,
            provider: None,
            content_kind: None,
            chunking_method: None,
            symbol_extraction_status: None,
        })
        .collect();

    Ok(QueryResult { results })
}

pub(super) fn normalize_time_bounds(
    cfg: &Config,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(Option<String>, Option<String>), Box<dyn Error>> {
    let since = normalize_time_bound(cfg.since.as_deref(), now, false)?;
    let before = normalize_time_bound(cfg.before.as_deref(), now, true)?;
    if let (Some(since), Some(before)) = (&since, &before) {
        let since_value = chrono::DateTime::parse_from_rfc3339(since)?;
        let before_value = chrono::DateTime::parse_from_rfc3339(before)?;
        if since_value > before_value {
            return Err(Box::new(ServiceError::new(
                "--since must not be later than --before".to_string(),
            )));
        }
    }
    Ok((since, before))
}

pub(super) fn normalize_time_bound(
    raw: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
    end_of_day_for_date: bool,
) -> Result<Option<String>, Box<dyn Error>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let timestamp = if let Some(days) = raw.strip_suffix('d').and_then(|v| v.parse::<i64>().ok()) {
        now - chrono::Duration::days(days)
    } else if let Some(weeks) = raw.strip_suffix('w').and_then(|v| v.parse::<i64>().ok()) {
        now - chrono::Duration::weeks(weeks)
    } else if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc();
        if end_of_day_for_date {
            midnight + chrono::Duration::days(1) - chrono::Duration::nanoseconds(1)
        } else {
            midnight
        }
    } else {
        chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|error| {
                ServiceError::new(format!(
                    "invalid temporal bound `{raw}`; expected Nd, Nw, YYYY-MM-DD, or RFC3339: {error}"
                ))
            })?
            .with_timezone(&chrono::Utc)
    };
    Ok(Some(timestamp.to_rfc3339()))
}

type ResolvedStores = (
    Arc<dyn VectorStore>,
    Arc<dyn EmbeddingProvider>,
    axon_api::source::ProviderId,
    String,
    u32,
);

/// Resolve the read-plane stores + provider identity, preferring the context's
/// attached runtime.
async fn resolve_stores(ctx: &ServiceContext, cfg: &Config) -> ResolvedStores {
    if let Some(target) = ctx.target_local_source_runtime() {
        return (
            Arc::clone(&target.vector_store),
            Arc::clone(&target.embedding_provider),
            target.embedding_provider_id.clone(),
            target.embedding_model.clone(),
            target.embedding_dimensions,
        );
    }
    let stores = build_read_stores_from_config(cfg).await;
    (
        stores.vector_store,
        stores.embedding_provider,
        stores.embedding_provider_id,
        stores.embedding_model,
        stores.embedding_dimensions,
    )
}

/// Derive a short display source (host, or the raw value) from a canonical URI.
fn display_source(uri: &str) -> String {
    reqwest::Url::parse(uri)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
        .unwrap_or_else(|| uri.to_string())
}

#[cfg(test)]
#[path = "retrieval_tests.rs"]
mod tests;
