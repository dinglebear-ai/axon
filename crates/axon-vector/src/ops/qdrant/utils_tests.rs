use axon_core::config::Config;

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
            ..QdrantPayload::default()
        },
    }
}

fn make_payload(chunk_text: &str, text: &str) -> QdrantPayload {
    QdrantPayload {
        url: String::new(),
        chunk_text: chunk_text.to_string(),
        text: text.to_string(),
        chunk_index: None,
        ..QdrantPayload::default()
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
