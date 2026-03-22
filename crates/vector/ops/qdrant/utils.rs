use super::types::{QdrantPayload, QdrantPoint, RETRIEVE_MAX_POINTS_CEILING};
use crate::crates::core::config::Config;
use spider::url::Url;
use std::env;
use std::sync::LazyLock;

pub fn qdrant_base(cfg: &Config) -> &str {
    cfg.qdrant_url.trim_end_matches('/')
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

pub fn render_full_doc_from_points(mut points: Vec<QdrantPoint>) -> String {
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
    use super::super::types::{QdrantPayload, QdrantPoint};
    use super::{
        RETRIEVE_MAX_POINTS_CEILING, base_url, env_usize_clamped, query_snippet,
        render_full_doc_from_points, retrieve_max_points,
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
}
