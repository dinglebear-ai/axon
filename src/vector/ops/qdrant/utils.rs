use super::types::{QdrantPayload, QdrantPoint, RETRIEVE_MAX_POINTS_CEILING};
use crate::core::config::{CollectionNameError, Config};
use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use rand::RngExt as _;
use reqwest::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use spider::url::Url;
use std::env;
use std::time::{Duration, Instant};

pub fn qdrant_base(cfg: &Config) -> &str {
    cfg.qdrant_url.trim_end_matches('/')
}

/// Current schema version written into every new Qdrant payload.
///
/// Existing points indexed before this constant was introduced have no
/// `payload_schema_version` field; treat them as implicit version `1`. New
/// upserts carry the current version (this constant). Retrieval may filter
/// `payload_schema_version >= N` to scope queries to vertical-aware fields
/// (see `build_schema_version_filter` in `qdrant/filter.rs`).
///
/// Bumped when a new required payload field lands. See bead `axon_rust-lu6a`.
/// v4: Promoted gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork,
///     gh_is_archived, gh_file_type, gh_line_start, gh_line_end from git_meta
///     blob to indexed top-level fields.
/// v5: Added indexed top-level `seed_url` — the crawl start URL or ingest target
///     that originated each chunk (origin tracking; consumed by `axon refresh`).
/// v6: Added code chunk `symbol_name`/`symbol_kind` metadata and restored
///     `chunking_method` writes for GitHub file chunks.
/// v7: Clean-break git/code payload schema. New git-backed file chunks write
///     provider-neutral `git_*` and `code_*` fields instead of `gh_*` duplicates.
/// v8: Added normalized pre-chunk planner fields: `chunk_content_kind`,
///     `chunk_locator`, `source_range`, `chunking_fallback`, and
///     `code_chunk_source`.
pub const PAYLOAD_SCHEMA_VERSION: u32 = 8;

pub(crate) fn validate_collection_name(name: &str) -> Result<(), CollectionNameError> {
    crate::core::config::validate_collection_name(name)
}

pub(crate) fn validate_config_collection(cfg: &Config) -> Result<()> {
    validate_collection_name(&cfg.collection).map_err(|reason| {
        anyhow!(
            "invalid collection name {:?}: {reason} (CWE-22 path injection guard)",
            cfg.collection
        )
    })
}

pub(crate) fn qdrant_collection_endpoint(cfg: &Config, suffix: &str) -> Result<String> {
    validate_config_collection(cfg)?;
    let suffix = suffix.trim_start_matches('/');
    Ok(format!(
        "{}/collections/{}/{}",
        qdrant_base(cfg),
        cfg.collection,
        suffix
    ))
}

/// Exponential backoff with jitter for Qdrant retries.
pub(crate) fn qdrant_retry_delay(attempt: usize) -> Duration {
    debug_assert!(attempt >= 1, "attempt must be >= 1");
    let base_ms = 250_u64.saturating_mul(1u64 << attempt.saturating_sub(1));
    let jitter_ms = rand::rng().random_range(0..100);
    Duration::from_millis(base_ms + jitter_ms)
}

pub(crate) async fn qdrant_post_json_with_retry<B, T>(
    client: &reqwest::Client,
    endpoint: &str,
    body: &B,
    context: &str,
    collection: &str,
    started: Instant,
) -> Result<T>
where
    B: Serialize + ?Sized,
    T: DeserializeOwned,
{
    const MAX_ATTEMPTS: usize = 4;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match client.post(endpoint).json(body).send().await {
            Ok(resp) => {
                let status = resp.status();
                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if retryable && attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "{context}: retrying qdrant request collection={collection} status={status} attempt={attempt}/{MAX_ATTEMPTS} duration_ms={}",
                        started.elapsed().as_millis()
                    ));
                    last_err = Some(anyhow!(
                        "{context}: qdrant status={status} attempt={attempt}"
                    ));
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                    continue;
                }
                let parsed = resp.error_for_status()?.json::<T>().await?;
                return Ok(parsed);
            }
            Err(err) => {
                if attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "{context}: retrying qdrant request collection={collection} transport_error attempt={attempt}/{MAX_ATTEMPTS} duration_ms={} err={err}",
                        started.elapsed().as_millis()
                    ));
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                }
                last_err = Some(err.into());
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("{context}: unknown qdrant request failure")))
}

pub(crate) fn env_usize_clamped(key: &str, default: usize, min: usize, max: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
        .clamp(min, max)
}

pub fn payload_text_typed(payload: &QdrantPayload) -> &str {
    if !payload.chunk_text.is_empty() {
        payload.chunk_text.as_str()
    } else {
        payload.text.as_str()
    }
}

pub fn payload_url_typed(payload: &QdrantPayload) -> &str {
    payload.url.as_str()
}

pub(crate) fn payload_url(payload: &serde_json::Value) -> String {
    payload
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn payload_domain(payload: &serde_json::Value) -> String {
    payload
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

pub fn base_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let mut out = format!("{}://{host}", parsed.scheme());
    if let Some(port) = parsed.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    Some(out)
}

pub fn render_full_doc_from_points(points: Vec<QdrantPoint>) -> String {
    render_full_doc_filtered(points, None, None)
}

/// Render full-doc context with optional query-relevance filtering.
///
/// When `query_tokens` is provided, each chunk is scored by lowercase token
/// hit count and only the top `top_k` chunks are kept. The kept chunks are
/// re-sorted by `chunk_index` for narrative coherence so the LLM still reads
/// the document in document order — just without the irrelevant interludes.
///
/// When `query_tokens` is `None`, behaves as before: all chunks concatenated
/// in `chunk_index` order. (bd axon_rust-0fz)
pub fn render_full_doc_filtered(
    points: Vec<QdrantPoint>,
    query_tokens: Option<&[String]>,
    top_k: Option<usize>,
) -> String {
    let kept: Vec<usize> = match (query_tokens, top_k) {
        (Some(tokens), Some(k)) if !tokens.is_empty() && k > 0 && points.len() > k => {
            let mut order = rank_points_by_query_overlap(&points, tokens);
            order.truncate(k);
            order
        }
        _ => (0..points.len()).collect(),
    };
    render_points_in_doc_order(&points, &kept)
}

/// Relevance order over `points` by query-token overlap, descending. The sort
/// is stable, so ties (including the all-zero case for empty tokens) preserve
/// enumeration order. Callers walk this once and slice prefixes instead of
/// re-scoring per candidate window.
pub fn rank_points_by_query_overlap(points: &[QdrantPoint], tokens: &[String]) -> Vec<usize> {
    let mut scored: Vec<(usize, f64)> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                i,
                score_chunk_against_tokens(payload_text_typed(&p.payload), tokens),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(idx, _)| idx).collect()
}

/// Render the chunks at `kept` indices re-sorted into `chunk_index` order for
/// narrative coherence — the LLM still reads the document in document order,
/// just without the irrelevant interludes. (bd axon_rust-0fz)
pub fn render_points_in_doc_order(points: &[QdrantPoint], kept: &[usize]) -> String {
    let mut selected: Vec<&QdrantPoint> = kept.iter().map(|&i| &points[i]).collect();
    selected.sort_by_key(|p| p.payload.chunk_index.unwrap_or(i64::MAX));
    let capacity = selected
        .iter()
        .map(|point| payload_text_typed(&point.payload).len())
        .sum::<usize>()
        + selected.len();
    let mut text = String::with_capacity(capacity);
    for point in selected {
        let chunk = payload_text_typed(&point.payload);
        if chunk.is_empty() {
            continue;
        }
        text.push_str(chunk);
        text.push('\n');
    }
    // Trim in place instead of allocating a new String via .trim().to_string().
    let trimmed_start = text.len() - text.trim_start().len();
    if trimmed_start > 0 {
        text.drain(..trimmed_start);
    }
    text.truncate(text.trim_end().len());
    text
}

/// Cheap query-overlap score: sum of token occurrences in the chunk's
/// lowercased text. Stable, no allocations beyond the lowercase pass.
fn score_chunk_against_tokens(chunk: &str, tokens: &[String]) -> f64 {
    let lower = chunk.to_ascii_lowercase();
    let mut score = 0.0_f64;
    for token in tokens {
        if token.is_empty() {
            continue;
        }
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = lower[start..].find(token.as_str()) {
            count += 1;
            start += pos + token.len();
        }
        score += count as f64;
    }
    score
}

pub fn query_snippet(payload: &QdrantPayload) -> String {
    let text = payload_text_typed(payload);
    // Truncate first, then replace newlines — avoids allocating the full 2KB string
    // just to discard everything past char 140.
    let end = text
        .char_indices()
        .nth(140)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    text[..end].replace('\n', " ")
}

pub(crate) fn retrieve_max_points(max_points: Option<usize>) -> usize {
    max_points
        .unwrap_or(RETRIEVE_MAX_POINTS_CEILING)
        .min(RETRIEVE_MAX_POINTS_CEILING)
}

#[cfg(test)]
#[allow(unsafe_code)]
#[path = "utils_tests.rs"]
mod tests;
