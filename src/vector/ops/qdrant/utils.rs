use super::types::{QdrantPayload, QdrantPoint, RETRIEVE_MAX_POINTS_CEILING};
use crate::core::config::{CollectionNameError, Config};
use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use rand::RngExt as _;
use reqwest::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use spider::url::Url;
use std::env;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub fn qdrant_base(cfg: &Config) -> &str {
    cfg.qdrant_url.trim_end_matches('/')
}

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

// ── Cached env vars for hot-path search operations ──────────────────────────
// These are read once at process startup via LazyLock instead of calling
// std::env::var() (which acquires a global lock) on every search request.

pub(crate) static HNSW_EF_SEARCH: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("AXON_HNSW_EF_SEARCH", 128, 32, 512));

pub(crate) static HNSW_EF_SEARCH_LEGACY: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("AXON_HNSW_EF_SEARCH_LEGACY", 64, 32, 512));

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
    mut points: Vec<QdrantPoint>,
    query_tokens: Option<&[String]>,
    top_k: Option<usize>,
) -> String {
    if let (Some(tokens), Some(k)) = (query_tokens, top_k)
        && !tokens.is_empty()
        && k > 0
        && points.len() > k
    {
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
        scored.truncate(k);
        let kept: std::collections::HashSet<usize> =
            scored.into_iter().map(|(idx, _)| idx).collect();
        let mut filtered = Vec::with_capacity(kept.len());
        for (i, p) in points.into_iter().enumerate() {
            if kept.contains(&i) {
                filtered.push(p);
            }
        }
        points = filtered;
    }

    points.sort_by_key(|p| p.payload.chunk_index.unwrap_or(i64::MAX));
    let capacity = points
        .iter()
        .map(|point| payload_text_typed(&point.payload).len())
        .sum::<usize>()
        + points.len();
    let mut text = String::with_capacity(capacity);
    for point in points {
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
mod tests {
    use crate::core::config::Config;

    use super::super::types::{QdrantPayload, QdrantPoint};
    use super::{
        RETRIEVE_MAX_POINTS_CEILING, base_url, env_usize_clamped, qdrant_collection_endpoint,
        query_snippet, render_full_doc_filtered, render_full_doc_from_points, retrieve_max_points,
        validate_collection_name,
    };

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_point(chunk_text: &str, text: &str, chunk_index: Option<i64>) -> QdrantPoint {
        QdrantPoint {
            id: serde_json::Value::Null,
            payload: QdrantPayload {
                url: String::new(),
                chunk_text: chunk_text.to_string(),
                text: text.to_string(),
                chunk_index,
            },
        }
    }

    fn make_payload(chunk_text: &str, text: &str) -> QdrantPayload {
        QdrantPayload {
            url: String::new(),
            chunk_text: chunk_text.to_string(),
            text: text.to_string(),
            chunk_index: None,
        }
    }

    // ── retrieve_max_points ───────────────────────────────────────────────────

    #[test]
    fn retrieve_max_points_defaults_to_ceiling() {
        assert_eq!(retrieve_max_points(None), RETRIEVE_MAX_POINTS_CEILING);
    }

    #[test]
    fn retrieve_max_points_caps_values_above_ceiling() {
        assert_eq!(
            retrieve_max_points(Some(RETRIEVE_MAX_POINTS_CEILING + 250)),
            RETRIEVE_MAX_POINTS_CEILING
        );
    }

    #[test]
    fn retrieve_max_points_preserves_lower_values() {
        assert_eq!(retrieve_max_points(Some(128)), 128);
    }

    #[test]
    fn collection_name_accepts_legal_values() {
        assert!(validate_collection_name("cortex").is_ok());
        assert!(validate_collection_name("axon_v2").is_ok());
        assert!(validate_collection_name("my-collection").is_ok());
        assert!(validate_collection_name("a.b.c").is_ok());
        assert!(validate_collection_name("a").is_ok());
    }

    #[test]
    fn collection_name_rejects_path_traversal() {
        assert!(validate_collection_name("..").is_err());
        assert!(validate_collection_name("../etc/passwd").is_err());
        assert!(validate_collection_name("a/b").is_err());
        assert!(validate_collection_name("a..b").is_err());
        assert!(validate_collection_name(".hidden").is_err());
        assert!(validate_collection_name("trailing.").is_err());
    }

    #[test]
    fn collection_name_rejects_url_delimiters() {
        assert!(validate_collection_name("a?x=1").is_err());
        assert!(validate_collection_name("a#frag").is_err());
        assert!(validate_collection_name("a b").is_err());
        assert!(validate_collection_name("a%2e%2e").is_err());
    }

    #[test]
    fn collection_name_rejects_empty_and_oversize() {
        assert!(validate_collection_name("").is_err());
        let huge = "a".repeat(256);
        assert!(validate_collection_name(&huge).is_err());
    }

    #[test]
    fn qdrant_collection_endpoint_validates_and_trims_suffix() {
        let mut cfg = Config::test_default();
        cfg.qdrant_url = "http://qdrant.local/".to_string();
        cfg.collection = "docs_v2".to_string();

        assert_eq!(
            qdrant_collection_endpoint(&cfg, "/points/scroll").unwrap(),
            "http://qdrant.local/collections/docs_v2/points/scroll"
        );

        cfg.collection = "docs/v2".to_string();
        assert!(qdrant_collection_endpoint(&cfg, "points/search").is_err());
    }

    // ── render_full_doc_from_points ───────────────────────────────────────────

    #[test]
    fn render_full_doc_empty_vec_returns_empty_string() {
        assert_eq!(render_full_doc_from_points(vec![]), "");
    }

    #[test]
    fn render_full_doc_single_chunk_renders_text() {
        let points = vec![make_point("hello world", "", Some(0))];
        assert_eq!(render_full_doc_from_points(points), "hello world");
    }

    #[test]
    fn render_full_doc_sorts_by_chunk_index_ascending() {
        // Supply chunks out of order; output must be ordered 0 → 1 → 2.
        let points = vec![
            make_point("second", "", Some(2)),
            make_point("first", "", Some(0)),
            make_point("middle", "", Some(1)),
        ];
        let result = render_full_doc_from_points(points);
        let pos_first = result.find("first").unwrap();
        let pos_middle = result.find("middle").unwrap();
        let pos_second = result.find("second").unwrap();
        assert!(pos_first < pos_middle, "first must come before middle");
        assert!(pos_middle < pos_second, "middle must come before second");
    }

    #[test]
    fn render_full_doc_none_chunk_index_comes_last() {
        let points = vec![
            make_point("no-index", "", None),
            make_point("indexed", "", Some(0)),
        ];
        let result = render_full_doc_from_points(points);
        let pos_indexed = result.find("indexed").unwrap();
        let pos_none = result.find("no-index").unwrap();
        assert!(
            pos_indexed < pos_none,
            "indexed chunk must appear before None chunk"
        );
    }

    #[test]
    fn render_full_doc_skips_empty_chunks() {
        // Both chunk_text and text are empty → the point is skipped entirely.
        let points = vec![
            make_point("", "", Some(0)),
            make_point("real content", "", Some(1)),
        ];
        let result = render_full_doc_from_points(points);
        assert_eq!(result, "real content");
    }

    #[test]
    fn render_full_doc_prefers_chunk_text_over_text() {
        // chunk_text is non-empty → it wins over text.
        let points = vec![make_point("preferred", "fallback", Some(0))];
        let result = render_full_doc_from_points(points);
        assert!(result.contains("preferred"), "chunk_text should be used");
        assert!(
            !result.contains("fallback"),
            "text should not appear when chunk_text is set"
        );
    }

    #[test]
    fn render_full_doc_falls_back_to_text_when_chunk_text_empty() {
        let points = vec![make_point("", "fallback text", Some(0))];
        assert_eq!(render_full_doc_from_points(points), "fallback text");
    }

    // ── query_snippet ─────────────────────────────────────────────────────────

    #[test]
    fn query_snippet_short_text_returned_in_full() {
        let payload = make_payload("short text", "");
        assert_eq!(query_snippet(&payload), "short text");
    }

    #[test]
    fn query_snippet_exactly_140_chars_returned_in_full() {
        let text = "a".repeat(140);
        let payload = make_payload(&text, "");
        let result = query_snippet(&payload);
        assert_eq!(result.len(), 140);
        assert_eq!(result, text);
    }

    #[test]
    fn query_snippet_longer_than_140_chars_truncated() {
        let text = "b".repeat(200);
        let payload = make_payload(&text, "");
        let result = query_snippet(&payload);
        assert_eq!(result.len(), 140);
    }

    #[test]
    fn query_snippet_newlines_replaced_with_spaces() {
        let payload = make_payload("line one\nline two\nline three", "");
        let result = query_snippet(&payload);
        assert!(
            !result.contains('\n'),
            "newlines must be replaced with spaces"
        );
        assert!(
            result.contains("line one line two"),
            "spaces should separate former lines"
        );
    }

    #[test]
    fn query_snippet_uses_chunk_text_over_text() {
        let payload = make_payload("chunk content", "text content");
        let result = query_snippet(&payload);
        assert!(result.contains("chunk content"));
        assert!(!result.contains("text content"));
    }

    // ── base_url ──────────────────────────────────────────────────────────────

    #[test]
    fn base_url_standard_https_url() {
        assert_eq!(
            base_url("https://example.com/some/path?q=1"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn base_url_with_non_standard_port() {
        assert_eq!(
            base_url("https://example.com:8443/path"),
            Some("https://example.com:8443".to_string())
        );
    }

    #[test]
    fn base_url_strips_path_keeps_scheme_and_host() {
        assert_eq!(
            base_url("https://docs.example.com/guide/intro"),
            Some("https://docs.example.com".to_string())
        );
    }

    #[test]
    fn base_url_invalid_url_returns_none() {
        assert_eq!(base_url("not a url at all ://???"), None);
    }

    // ── env_usize_clamped ─────────────────────────────────────────────────────

    #[test]
    fn env_usize_clamped_missing_key_returns_default() {
        // Use a key that is guaranteed to never be set in any environment.
        let val = env_usize_clamped("TEST_AXON_UTILS_CLAMP_MISSING_XYZ_1", 42, 1, 100);
        assert_eq!(val, 42);
    }

    #[test]
    fn env_usize_clamped_within_range_returns_value() {
        // SAFETY: unique key name; no other test touches this var.
        unsafe { std::env::set_var("TEST_AXON_UTILS_CLAMP_2", "50") };
        let val = env_usize_clamped("TEST_AXON_UTILS_CLAMP_2", 10, 1, 100);
        unsafe { std::env::remove_var("TEST_AXON_UTILS_CLAMP_2") };
        assert_eq!(val, 50);
    }

    #[test]
    fn env_usize_clamped_above_max_clamped_to_max() {
        // SAFETY: unique key name; no other test touches this var.
        unsafe { std::env::set_var("TEST_AXON_UTILS_CLAMP_3", "9999") };
        let val = env_usize_clamped("TEST_AXON_UTILS_CLAMP_3", 10, 1, 100);
        unsafe { std::env::remove_var("TEST_AXON_UTILS_CLAMP_3") };
        assert_eq!(val, 100);
    }

    #[test]
    fn env_usize_clamped_below_min_returns_default() {
        // `.filter(|v| *v >= min)` drops the parsed value; `unwrap_or(default)` fires;
        // `clamp(min, max)` then bounds-checks the default (10 >= 5, so stays 10).
        // SAFETY: unique key name; no other test touches this var.
        unsafe { std::env::set_var("TEST_AXON_UTILS_CLAMP_4", "2") };
        let val = env_usize_clamped("TEST_AXON_UTILS_CLAMP_4", 10, 5, 100);
        unsafe { std::env::remove_var("TEST_AXON_UTILS_CLAMP_4") };
        assert_eq!(val, 10);
    }

    #[test]
    fn env_usize_clamped_non_numeric_returns_default() {
        // SAFETY: unique key name; no other test touches this var.
        unsafe { std::env::set_var("TEST_AXON_UTILS_CLAMP_5", "not_a_number") };
        let val = env_usize_clamped("TEST_AXON_UTILS_CLAMP_5", 7, 1, 100);
        unsafe { std::env::remove_var("TEST_AXON_UTILS_CLAMP_5") };
        assert_eq!(val, 7);
    }

    // ── render_full_doc_filtered ──────────────────────────────────────────────

    #[test]
    fn render_filtered_keeps_top_k_by_query_overlap() {
        // 3 chunks, top_k=2. Chunks 0 + 2 contain query tokens; chunk 1 has none.
        // Expect chunks 0 and 2 only, in chunk_index order. (bd axon_rust-0fz)
        let points = vec![
            make_point("alpha bravo charlie", "", Some(0)),
            make_point("nothing useful here", "", Some(1)),
            make_point("alpha foxtrot golf", "", Some(2)),
        ];
        let tokens = vec!["alpha".to_string()];
        let result = render_full_doc_filtered(points, Some(&tokens), Some(2));
        assert!(result.contains("alpha bravo charlie"));
        assert!(result.contains("alpha foxtrot golf"));
        assert!(!result.contains("nothing useful"));
    }

    #[test]
    fn render_filtered_no_query_keeps_all_chunks() {
        // query_tokens=None → behaves like the legacy render (no filtering).
        let points = vec![
            make_point("first chunk", "", Some(0)),
            make_point("second chunk", "", Some(1)),
        ];
        let result = render_full_doc_filtered(points, None, Some(1));
        assert!(result.contains("first chunk"));
        assert!(result.contains("second chunk"));
    }

    #[test]
    fn render_filtered_re_sorts_kept_by_chunk_index() {
        // Even though the query-score order is chunk 2 > chunk 0, the rendered
        // text must appear in chunk_index order so the LLM reads document flow.
        let points = vec![
            make_point("alpha appears once here", "", Some(0)),
            make_point("alpha alpha alpha hits", "", Some(2)),
            make_point("noise", "", Some(1)),
        ];
        let tokens = vec!["alpha".to_string()];
        let result = render_full_doc_filtered(points, Some(&tokens), Some(2));
        let pos_once = result.find("once here").unwrap();
        let pos_hits = result.find("hits").unwrap();
        assert!(
            pos_once < pos_hits,
            "kept chunks must be re-sorted by chunk_index ascending"
        );
    }

    #[test]
    fn render_filtered_top_k_larger_than_input_keeps_all() {
        let points = vec![
            make_point("alpha", "", Some(0)),
            make_point("beta", "", Some(1)),
        ];
        let tokens = vec!["zzz".to_string()]; // no matches — but k > len, so no filter applied
        let result = render_full_doc_filtered(points, Some(&tokens), Some(10));
        assert!(result.contains("alpha"));
        assert!(result.contains("beta"));
    }
}
