use crate::crates::jobs::common::test_config;
use crate::crates::vector::ops::tei::tei_client::tei_embed;
use httpmock::{HttpMockResponse, MockServer};
use std::env;
use std::sync::{Arc, Mutex};

/// Guard that restores (or removes) an env var on drop.
///
/// Use only inside tests annotated with `#[serial_test::serial]` to prevent concurrent
/// env mutation across test threads.
struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}
impl EnvGuard {
    #[allow(unsafe_code)]
    fn set(key: &'static str, value: &str) -> Self {
        let original = env::var(key).ok();
        // SAFETY: caller must hold the serial_test lock (see #[serial] annotation) so no
        // other test thread reads or writes env vars concurrently.
        unsafe { env::set_var(key, value) };
        EnvGuard { key, original }
    }
}
impl Drop for EnvGuard {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: same serial_test lock guarantees exclusive env access for the duration
        // of the test and its cleanup.
        unsafe {
            match &self.original {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }
}

/// Empty input slice must short-circuit before any HTTP call.
#[tokio::test]
async fn tei_embed_empty_input_returns_empty_vec() {
    // Port 1 is unreachable — any HTTP attempt would cause the test to fail.
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.tei_url = "http://127.0.0.1:1".to_string();

    let result = tei_embed(&cfg, &[]).await.unwrap();
    assert!(
        result.is_empty(),
        "empty input must return empty vec without HTTP call"
    );
}

/// On a 429 response, tei_embed must retry and succeed on the second attempt.
#[tokio::test]
async fn tei_embed_retries_on_429() {
    let server = MockServer::start_async().await;
    let call_count = Arc::new(Mutex::new(0usize));
    let cc = Arc::clone(&call_count);

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST).path("/embed");
            then.respond_with(move |_req: &httpmock::HttpMockRequest| {
                let mut count = cc.lock().unwrap();
                *count += 1;
                if *count == 1 {
                    // First call: rate-limited.
                    HttpMockResponse::builder().status(429).build()
                } else {
                    // Retry: success with one embedding vector.
                    HttpMockResponse::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body("[[0.1,0.2,0.3,0.4]]")
                        .build()
                }
            });
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.tei_url = server.base_url();

    let inputs = vec!["hello world".to_string()];
    let result = tei_embed(&cfg, &inputs)
        .await
        .expect("tei_embed must succeed after retry");
    assert_eq!(
        result.len(),
        1,
        "must return one vector after 429→retry→200"
    );
}

/// On a 413 response, tei_embed must split the batch and re-request each half.
#[tokio::test]
async fn tei_embed_splits_batch_on_413() {
    let server = MockServer::start_async().await;

    // Specific mock (registered first = higher precedence in httpmock): 413 when BOTH items are
    // present in the body. Single-item requests only contain one string, so they fall through.
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/embed")
                .body_includes("input-alpha")
                .body_includes("input-beta");
            then.status(413);
        })
        .await;

    // Fallback mock (registered second = lower precedence): 200 for single-item calls.
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1f32, 0.2, 0.3, 0.4]]));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.tei_url = server.base_url();

    let inputs = vec!["input-alpha".to_string(), "input-beta".to_string()];
    let result = tei_embed(&cfg, &inputs)
        .await
        .expect("tei_embed must succeed after batch split");
    assert_eq!(
        result.len(),
        2,
        "must return two vectors (one per item after split)"
    );
}

/// Non-success HTTP responses (not just 429/503) should also retry.
#[serial_test::serial]
#[tokio::test]
async fn tei_embed_retries_on_500() {
    let server = MockServer::start_async().await;
    let call_count = Arc::new(Mutex::new(0usize));
    let cc = Arc::clone(&call_count);

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST).path("/embed");
            then.respond_with(move |_req: &httpmock::HttpMockRequest| {
                let mut count = cc.lock().unwrap();
                *count += 1;
                if *count == 1 {
                    HttpMockResponse::builder()
                        .status(500)
                        .body("temporary failure")
                        .build()
                } else {
                    HttpMockResponse::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body("[[0.9,0.8,0.7,0.6]]")
                        .build()
                }
            });
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.tei_url = server.base_url();

    // Pin retry-related env vars so ambient overrides can't change test behavior.
    let _retry_guard = EnvGuard::set("TEI_MAX_RETRIES", "5");
    let _timeout_guard = EnvGuard::set("TEI_REQUEST_TIMEOUT_MS", "10000");

    let inputs = vec!["retry-on-500".to_string()];
    let result = tei_embed(&cfg, &inputs)
        .await
        .expect("tei_embed must succeed after retry on 500");
    assert_eq!(result.len(), 1, "must return one embedding vector");
}

/// Compile-time check: PreparedDoc must expose all four ingest metadata fields.
#[test]
fn prepared_doc_with_ingest_metadata_compiles() {
    // Compile-time check: all four new fields must exist on PreparedDoc.
    // This test FAILS before Step 3 adds them (unknown field errors).
    let doc = super::PreparedDoc {
        url: "https://github.com/owner/repo/blob/main/src/lib.rs".to_string(),
        domain: "github.com".to_string(),
        chunks: vec!["fn main() {}".to_string()],
        source_type: "github".to_string(),
        content_type: "text",
        title: Some("src/lib.rs".to_string()),
        extra: Some(serde_json::json!({"gh_owner": "owner", "gh_repo": "repo"})),
    };
    assert_eq!(doc.source_type, "github");
    assert_eq!(doc.content_type, "text");
    assert!(doc.title.is_some());
    assert!(doc.extra.is_some());
}

/// Hard client errors should fail fast (no retry storm).
#[tokio::test]
async fn tei_embed_fails_fast_on_404() {
    let server = MockServer::start_async().await;
    let call_count = Arc::new(Mutex::new(0usize));
    let cc = Arc::clone(&call_count);

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST).path("/embed");
            then.respond_with(move |_req: &httpmock::HttpMockRequest| {
                let mut count = cc.lock().unwrap();
                *count += 1;
                HttpMockResponse::builder()
                    .status(404)
                    .body("not found")
                    .build()
            });
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.tei_url = server.base_url();

    let inputs = vec!["fail-fast-404".to_string()];
    let err = tei_embed(&cfg, &inputs)
        .await
        .expect_err("tei_embed must fail fast on 404");
    let msg = err.to_string();
    assert!(
        msg.contains("status 404"),
        "unexpected error message: {msg}"
    );
    assert_eq!(*call_count.lock().unwrap(), 1, "404 should not be retried");
}

/// EmbedSummary must expose the docs_failed field for partial-result reporting.
#[test]
fn embed_summary_exposes_docs_failed() {
    let summary = super::EmbedSummary {
        docs_embedded: 10,
        docs_failed: 3,
        chunks_embedded: 42,
    };
    assert_eq!(summary.docs_embedded, 10);
    assert_eq!(summary.docs_failed, 3);
    assert_eq!(summary.chunks_embedded, 42);
}

/// TEI retry default (5) must produce a worst-case retry budget that fits
/// inside the doc timeout (300s default).
#[test]
fn tei_max_retries_default_fits_doc_timeout() {
    let max_retries = 5usize;
    let request_timeout_s = 30u64;
    let backoff_sum_s: u64 = (0..max_retries as u32)
        .map(|i| 1u64.saturating_mul(2u64.pow(i)).min(60))
        .sum();
    let worst_case = (max_retries as u64 * request_timeout_s) + backoff_sum_s;
    assert!(
        worst_case < 300,
        "worst-case retry budget ({worst_case}s) must fit inside 300s doc timeout"
    );
}

// -- build_point format tests --

#[test]
fn build_point_unnamed_emits_flat_vector() {
    use super::qdrant_store::VectorMode;
    let point = super::build_point_for_test(
        vec![0.1, 0.2, 0.3],
        "hello world",
        "https://example.com/page",
        0,
        VectorMode::Unnamed,
    );
    // Unnamed mode: vector is a flat array, not an object
    assert!(
        point["vector"].is_array(),
        "Unnamed mode must emit flat vector array, got: {}",
        point["vector"]
    );
    assert_eq!(point["vector"].as_array().unwrap().len(), 3);
}

#[test]
fn build_point_named_emits_dense_and_bm42() {
    use super::qdrant_store::VectorMode;
    let point = super::build_point_for_test(
        vec![0.1, 0.2, 0.3],
        "qdrant vector search embedding",
        "https://example.com/page",
        0,
        VectorMode::Named,
    );
    // Named mode: vector is an object with "dense" and "bm42" keys
    assert!(
        point["vector"].is_object(),
        "Named mode must emit vector object, got: {}",
        point["vector"]
    );
    assert!(
        point["vector"]["dense"].is_array(),
        "Named mode must have 'dense' key"
    );
    assert!(
        point["vector"]["bm42"].is_object(),
        "Named mode must have 'bm42' key with indices/values"
    );
    assert!(point["vector"]["bm42"]["indices"].is_array());
    assert!(point["vector"]["bm42"]["values"].is_array());
}

#[test]
fn build_point_named_sparse_has_nonzero_entries_for_real_text() {
    use super::qdrant_store::VectorMode;
    let point = super::build_point_for_test(
        vec![0.5, 0.6],
        "rust programming language systems performance",
        "https://rust-lang.org/learn",
        1,
        VectorMode::Named,
    );
    let indices = point["vector"]["bm42"]["indices"]
        .as_array()
        .expect("bm42 indices");
    assert!(
        !indices.is_empty(),
        "real text must produce non-empty sparse vector"
    );
}
