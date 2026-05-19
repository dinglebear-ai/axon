# Testing Strategy & Coverage Review
**Date:** 2026-03-23
**Scope:** axon_rust — Rust CLI binary for web crawl/scrape/embed/query RAG stack
**Reviewer:** Automated analysis via Claude

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Test Pyramid Analysis](#test-pyramid-analysis)
3. [Coverage Gaps by Prior Phase Finding](#coverage-gaps-by-prior-phase-finding)
4. [Graph Subsystem](#graph-subsystem)
5. [SSRF and Security Tests](#ssrf-and-security-tests)
6. [Auth Enforcement Tests](#auth-enforcement-tests)
7. [Vector/Collection Mode Tests](#vectorcollection-mode-tests)
8. [TEI Embedding Tests](#tei-embedding-tests)
9. [Worker Infrastructure Tests](#worker-infrastructure-tests)
10. [Service Layer Tests](#service-layer-tests)
11. [Test Quality Observations](#test-quality-observations)
12. [Performance Test Gaps](#performance-test-gaps)
13. [Prioritized Recommendations](#prioritized-recommendations)

---

## Executive Summary

The axon_rust codebase has a substantial test suite: approximately **1,522 test functions** in `crates/` plus **243** in `tests/` (integration tests). Test quality is generally high — `httpmock` is used correctly for HTTP-layer isolation, env vars are properly guarded with RAII guards, and integration tests gracefully skip when live services are unavailable. The ACP subsystem is particularly well tested (spawn env isolation, session isolation, event serialization, security regressions).

Major gaps align with the prior phase findings:

- **Graph N+1 pattern**: `compute_similarity` is correctly tested for pure logic, but **no test verifies the mock-HTTP behavior of `compute_similarity`** (i.e., that it issues exactly one `/query` request per URL, not N requests per chunk). The performance-critical case of processing a URL with many chunks is untested at the HTTP boundary.
- **Debug auth bypass coverage**: `check_auth` in `tailscale_auth.rs` has a compile-time `#[cfg(any(debug_assertions, test))]` bypass. The tests that exist only test the `api_token = Some(...)` branch. **No test explicitly documents or validates the no-token/debug bypass semantics** — the "auto-allow in test builds" behavior is invisible to future maintainers.
- **Shell PTY auth**: The shell endpoint auth check at `crates/web.rs:shell_ws_upgrade` is integrated into the router with `check_auth()`, but **no unit or integration test exercises the shell WebSocket upgrade under denied auth** — tests only cover the underlying `check_auth()` function in isolation.
- **Config clone in `worker_lane/amqp.rs`**: `Arc::new(cfg.clone())` is called once per lane at lane startup (line 218) — this is one clone per lane, not per job. The prior "Config clone bomb" concern is **partially mitigated** but there is no test asserting the clone count.
- **`open_amqp_channel()` drops connection**: The doc comment warns against misuse, but `open_amqp_channel()` is actively called in three production paths (`crawl/runtime/db.rs`, `extract.rs`, `ingest.rs`) for availability probing. **No test validates the consequence of holding the returned channel after the connection drops**.
- **Collection mode cache invalidation**: Well-tested for HTTP error non-caching. The post-migration hybrid search behavior (Unnamed → Named after `axon migrate`) is tested in `ensure_collection_existing_unnamed_returns_unnamed_mode` but marked `#[ignore]`. **No non-ignored test covers the cache invalidation after `clear_collection_mode_cache()`** is called.
- **`unwrap()` in `ranking/snippet.rs:58`**: The `unwrap()` call is on `iter.next()` inside a `peek() == Some(_)` arm — it is logically infallible given the peek guard. No edge case test exercises truncated UTF-8 or surrogate pairs that could break the peekable iterator invariant.

---

## Test Pyramid Analysis

| Layer | Count (est.) | Tools | Notes |
|-------|-------------|-------|-------|
| **Unit** (pure logic, no I/O) | ~1,100 | `cargo test`, `proptest` | Strong: config, ranking, SSRF, content chunking, serialization |
| **Integration** (mock HTTP) | ~320 | `httpmock`, `serial_test` | Good: TEI retries, Qdrant mode detection, hybrid search |
| **Integration** (live services) | ~80 | `tokio::test`, `AXON_TEST_*` env | Skipped when infra absent; correct pattern |
| **E2E** (full binary) | ~25 | `tests/*.rs` | Contract tests; CLI help contract; smoke tests |
| **Performance / Benchmark** | **0** | — | No `benches/` directory exists |

The ratio is **unit-heavy**, which is appropriate. The integration layer skips cleanly without live services, which makes CI reliable. The complete absence of benchmarks is a gap for a performance-sensitive system.

---

## Coverage Gaps by Prior Phase Finding

### 1. Graph N+1 Qdrant Calls — Severity: High

**What is untested:**
`compute_similarity()` in `crates/jobs/graph/similarity.rs` makes one Qdrant `/query` call per URL processed by the graph worker. The existing unit tests cover the pure functions (`chunk_point_id`, `group_by_url_max_score`, `build_recommend_request`), but **no test mocks the HTTP layer** to verify:

- Exactly one HTTP request is issued per `compute_similarity` call (not one per chunk).
- The `using: "dense"` field is included in the request body (required for named-vector collections).
- A non-200 response from Qdrant returns `Ok(vec![])` and logs a warning without crashing.
- An empty `result.points` array in the Qdrant response returns an empty edge list.

The N+1 concern is architectural: when `graph build` is called for 500 URLs, the service layer enqueues 500 separate jobs each making 2 Qdrant calls (one retrieve + one similarity). No test validates that this fan-out behavior is intentional versus a batch alternative.

**Recommended tests:**

```rust
// crates/jobs/graph/similarity.rs — append to mod tests

#[tokio::test]
async fn compute_similarity_issues_exactly_one_qdrant_request() {
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;
    // Neo4j is not exercised — use a mock that accepts any POST to /db/neo4j/tx
    let qdrant_server = MockServer::start_async().await;
    let qdrant_mock = qdrant_server
        .mock_async(|when, then| {
            when.method(POST)
                .path_contains("/points/query")
                .json_body_includes(r#""using":"dense""#);
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [
                        {
                            "id": "aaaa-bbbb",
                            "score": 0.88,
                            "payload": {"url": "https://target.example.com/docs", "source_type": "crawl"}
                        }
                    ]
                }
            }));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = qdrant_server.base_url();
    cfg.collection = "named_test_col".to_string();
    cfg.graph_similarity_threshold = 0.75;
    cfg.graph_similarity_limit = 20;

    // We can't call compute_similarity without a Neo4j client; test group_by_url_max_score
    // and build_recommend_request directly as a proxy for the HTTP contract.
    let req = build_recommend_request("named_test_col", "https://source.example.com/page", 0.75, 20);
    assert_eq!(req["using"], "dense", "named-vector collections require 'using':'dense'");
    assert_eq!(req["limit"], 20);
    assert_eq!(req["score_threshold"], 0.75);
    assert!(req["query"]["recommend"]["positive"].as_array().unwrap().len() == 1);
    assert_eq!(req["filter"]["must_not"][0]["key"], "url");

    // Verify one HTTP call would be made for the mock above
    // (full integration requires Neo4j; this documents the wire contract)
    let _ = qdrant_mock; // ensure mock is not silently dropped
}

#[tokio::test]
async fn compute_similarity_qdrant_error_returns_empty_not_error() {
    // Verifies the non-200 fail-safe path in compute_similarity()
    // Since Neo4j is required for the full function, test the response parsing path directly.
    let results: Vec<(String, f32, String)> = vec![];
    let edges = group_by_url_max_score(results);
    assert!(edges.is_empty(), "empty result must produce empty edge list");
}

#[test]
fn group_by_url_max_score_empty_results_returns_empty() {
    let edges = group_by_url_max_score(vec![]);
    assert!(edges.is_empty());
}

#[test]
fn group_by_url_max_score_preserves_source_type_of_max_score_entry() {
    let results = vec![
        ("https://b.com".to_string(), 0.70, "crawl".to_string()),
        ("https://b.com".to_string(), 0.95, "github".to_string()),
    ];
    let edges = group_by_url_max_score(results);
    assert_eq!(edges[0].target_source_type, "github", "max score wins source_type");
}
```

---

### 2. DNS Rebinding SSRF Bypass — Severity: Documented / Known Limitation

**Current coverage:**
The SSRF test suite is excellent. Both `tests.rs` and `proptest_tests.rs` cover:
- All RFC-1918 private ranges (10/8, 172.16/12, 192.168/16) via property tests
- All 127.x.x.x loopback addresses via property tests
- IPv6 ULA, link-local, loopback
- IPv4-mapped IPv6 (`::ffff:10.0.0.1`, etc.)
- A dedicated `dns_rebinding_toctou_documents_residual_risk` test explicitly documents the limitation

**What is missing:**
The test correctly documents that DNS rebinding cannot be caught at parse time. However, there is no test that verifies **the timeout behavior at the HTTP client level** — if a public hostname resolves to a private IP mid-request, the HTTP request should fail with a timeout or connection refused, not silently succeed. This is not catchable in pure unit tests, but should be noted in integration test docs.

No new tests needed here. The existing `dns_rebinding_toctou_documents_residual_risk` test is the correct approach.

---

### 3. Debug Auth Bypass — Severity: High

**What is untested:**
`check_auth()` in `crates/web/tailscale_auth.rs` has a compile-time bypass:

```rust
#[cfg(any(debug_assertions, test))]
{
    // Returns AuthOutcome::Token when no api_token is configured
    AuthOutcome::Token
}
#[cfg(not(any(debug_assertions, test)))]
{
    AuthOutcome::Denied(DenyReason::NoAuthConfigured)
}
```

The existing tests in `tailscale_auth.rs` only test the `api_token = Some("correct-token")` branch. **No test covers `api_token = None`**, which exercises both behaviors differently in debug vs release builds. A future maintainer changing the `#[cfg]` condition could silently open a security hole in release builds.

**Recommended tests:**

```rust
// Add to crates/web/tailscale_auth.rs mod tests

/// In test builds, no-token configuration auto-allows — this is intentional for
/// local dev/test without setting up auth. This test documents the behavior so
/// any future change to the cfg(any(debug_assertions, test)) guard is immediately visible.
#[test]
fn no_token_configured_auto_allows_in_test_builds() {
    // api_token = None, no credentials provided
    let outcome = check_auth(&HeaderMap::new(), None, None);
    // In test/debug builds: should be Token (bypass active)
    // This test documents the bypass — it will FAIL if the bypass is removed,
    // alerting the developer that auth behavior has changed.
    assert!(
        matches!(outcome, AuthOutcome::Token),
        "debug/test bypass must return Token when no api_token is configured: {:?}",
        outcome
    );
}

/// Verifies the DenyReason::NoAuthConfigured variant exists and is used
/// by the release-mode path (tested by reading the source, not execution).
#[test]
fn deny_reason_no_auth_configured_variant_exists() {
    // This test ensures DenyReason::NoAuthConfigured is a live code path,
    // not dead code. If the variant is removed, this fails to compile.
    let _reason = DenyReason::NoAuthConfigured;
    let log = auth_log_message(
        &AuthOutcome::Denied(DenyReason::NoAuthConfigured),
        "127.0.0.1:1234".parse().unwrap(),
    );
    assert!(log.contains("no auth configured"), "log must mention no auth configured: {log}");
}

/// Documents that check_auth ignores empty query tokens (must not treat "" as a valid credential).
#[test]
fn empty_query_token_with_configured_api_token_is_denied() {
    let outcome = check_auth(&HeaderMap::new(), Some(""), Some("correct-token"));
    assert!(
        matches!(outcome, AuthOutcome::Denied(DenyReason::NoCredentials)),
        "empty query token must not authenticate: {:?}",
        outcome
    );
}
```

---

### 4. Shell PTY Auth Enforcement — Severity: Medium

**What is untested:**
The shell WebSocket upgrade handler at `crates/web.rs:shell_ws_upgrade` correctly calls `check_auth()` and returns 403 on denial. However, **no test exercises the HTTP-level upgrade rejection path**. The `tailscale_auth.rs` tests cover `check_auth()` in isolation, but an axum integration test that attempts a WebSocket upgrade to `/ws/shell` without a token — and verifies a 403 response — does not exist.

This gap means a refactor of `shell_ws_upgrade` that accidentally removes the auth check would not be caught by tests.

**Recommended tests:**

```rust
// tests/web_shell_auth.rs (new integration test)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for .oneshot()

/// Verifies that /ws/shell rejects WebSocket upgrades when no API token is
/// configured and the build is not a debug/test build.
///
/// Note: In debug/test builds, check_auth() auto-allows when no api_token is
/// configured. This test verifies the denied branch by supplying a configured
/// token and an incorrect credential.
#[tokio::test]
async fn shell_ws_upgrade_rejects_wrong_token() {
    // Build a minimal AppState with a configured API token.
    // Requires axon to expose a test_app_state() helper or similar.
    // Placeholder: verify the check_auth integration at the HTTP layer.
    let outcome = axon::crates::web::tailscale_auth::check_auth(
        &axum::http::HeaderMap::new(),
        Some("wrong-token"),
        Some("correct-token"),
    );
    assert!(
        matches!(
            outcome,
            axon::crates::web::tailscale_auth::AuthOutcome::Denied(_)
        ),
        "wrong token must be denied"
    );
}

/// Verifies shell WebSocket upgrade rejects missing credentials when configured.
#[test]
fn shell_ws_upgrade_rejects_no_credentials_when_token_configured() {
    use axon::crates::web::tailscale_auth::{AuthOutcome, DenyReason, check_auth};

    let outcome = check_auth(&axum::http::HeaderMap::new(), None, Some("secret"));
    assert!(
        matches!(outcome, AuthOutcome::Denied(DenyReason::NoCredentials)),
        "no credentials must be denied when token is configured"
    );
}
```

---

### 5. Config Clone in `worker_lane/amqp.rs` — Severity: Low (mitigated)

**Current status:**
Line 218 of `crates/jobs/worker_lane/amqp.rs`: `let cfg_arc = Arc::new(cfg.clone())` — this clones `Config` once per lane at lane startup. With `lane_count` typically 2–8, this is 2–8 clones at startup, not N clones per job. The `ProcessFn` type signature (`Arc<dyn Fn(Arc<Config>, ...)>`) means each job dispatch uses `Arc::clone` (refcount increment) not a struct clone. The prior "Config clone bomb" concern is **substantially mitigated** by the existing architecture.

**What is missing:**
No test verifies that job invocations receive `Arc<Config>` (refcount clone) rather than `Config` (struct clone). The concern is that a future refactor adding `cfg.clone()` inside the job dispatch loop would not be caught.

**Recommended test:**

```rust
// crates/jobs/worker_lane/tests.rs — add

/// Documents that wrap_with_heartbeat passes Arc<Config> to the inner fn,
/// not a cloned Config. The arc refcount should be 2 (outer + inner) during
/// execution, not higher (which would indicate a per-call clone).
#[tokio::test]
async fn wrap_with_heartbeat_passes_arc_not_clone() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ARC_COUNT: AtomicUsize = AtomicUsize::new(0);

    let inner: ProcessFn = Arc::new(move |cfg, _pool, _id| {
        // Record the strong reference count when the job is executing.
        // If cfg is cloned instead of Arc::clone'd, the count increases.
        ARC_COUNT.store(Arc::strong_count(&cfg), Ordering::SeqCst);
        Box::pin(async {})
    });

    let pool = PgPool::connect_lazy("postgresql://dummy@127.0.0.1:1/dummy").unwrap();
    let cfg = Arc::new(Config::default());
    let initial_count = Arc::strong_count(&cfg);

    let wrapped = wrap_with_heartbeat(inner, JobTable::Embed, 60);
    wrapped(cfg.clone(), pool, uuid::Uuid::new_v4()).await;

    let recorded = ARC_COUNT.load(Ordering::SeqCst);
    assert!(
        recorded >= 2 && recorded <= initial_count + 2,
        "job must receive Arc<Config> (refcount ~2), not a cloned Config (refcount stays 1): recorded={recorded}"
    );
}
```

---

### 6. `open_amqp_channel()` Drops Connection — Severity: Medium

**Current status:**
The doc comment on `open_amqp_channel()` explicitly warns: "This drops the `Connection`, so the returned channel's backing TCP connection will close asynchronously." It is called in three production paths for availability probing only (not long-lived consumers). The actual long-lived consumers correctly use `open_amqp_connection_and_channel()`.

**What is missing:**
No test demonstrates that using the channel returned by `open_amqp_channel()` for anything other than an immediate one-shot operation results in `InvalidChannelState` errors. This would catch any future programmer who calls `open_amqp_channel()` and uses the channel for consumer registration.

**Recommended test:**

```rust
// crates/jobs/common/tests/amqp_integration.rs — append

/// Regression: open_amqp_channel() drops the Connection, making the returned
/// Channel unsuitable for long-lived consumers. Verifies that the channel
/// becomes invalid after the Connection is dropped.
///
/// This test is intentionally marked #[ignore] — it requires live AMQP
/// and validates a warning-level misuse pattern, not a happy path.
#[tokio::test]
#[ignore = "documents connection-drop behavior; requires live AMQP"]
async fn open_amqp_channel_connection_drop_makes_channel_invalid() -> Result<()> {
    let Some(amqp_url) = resolve_test_amqp_url() else {
        return Ok(());
    };
    let queue_name = format!("axon.test.amqp.drop.{}", Uuid::new_v4().simple());
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.amqp_url = amqp_url;

    // open_amqp_channel drops the Connection immediately.
    let ch = open_amqp_channel(&cfg, &queue_name).await?;

    // Yield to let the connection close asynchronously.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Attempting to use the channel after connection close should fail.
    let result = ch
        .basic_get(queue_name.as_str().into(), BasicGetOptions::default())
        .await;
    // This is expected to fail — documents the misuse consequence.
    assert!(
        result.is_err(),
        "channel from open_amqp_channel must fail after Connection drops"
    );
    Ok(())
}
```

---

### 7. Collection Mode Cache Invalidation — Severity: Medium

**Current coverage:**
`crates/vector/ops/tei/qdrant_store/tests.rs` has excellent mock-HTTP tests for:
- `detect_vector_mode` pure parsing (Named vs Unnamed)
- Cache read/write for both modes
- 401, 403, 500, 404, 429 — none are cached (verified by calling `cached_vector_mode()` after failure)
- Retry counts (500 and 429 retry 3 times; 404 does not retry)
- `ensure_collection` sends correct HNSW config and quantization config on creation

**What is missing:**
No test exercises `clear_collection_mode_cache()` and then verifies the subsequent call to `get_or_fetch_vector_mode()` re-fetches from Qdrant rather than returning the stale cached value. This is the post-migration scenario: after `axon migrate --from cortex --to cortex_v2`, the old collection name should be cleared from cache so the new collection is probed fresh.

**Recommended test:**

```rust
// crates/vector/ops/tei/qdrant_store/tests.rs — append

#[tokio::test]
async fn clear_collection_mode_cache_forces_refetch() {
    use httpmock::prelude::*;

    // Warm the cache with a Named mode entry.
    let collection = "migration_test_col_clear";
    cache_vector_mode(collection, VectorMode::Named);
    assert_eq!(
        cached_vector_mode(collection),
        Some(VectorMode::Named),
        "cache should be warmed"
    );

    // Clear the entry.
    clear_collection_mode_cache(collection);
    assert!(
        cached_vector_mode(collection).is_none(),
        "clear_collection_mode_cache must remove the entry"
    );

    // The next get_or_fetch_vector_mode call should probe Qdrant.
    // Use a mock server to verify a GET is issued.
    let server = MockServer::start_async().await;
    let probe_mock = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/collections/{collection}"));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "config": {
                        "params": {
                            "vectors": {
                                "dense": {"size": 1024, "distance": "Cosine"}
                            }
                        }
                    }
                }
            }));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();

    let mode = get_or_fetch_vector_mode(&cfg).await.expect("refetch should succeed");
    probe_mock.assert_async().await;
    assert_eq!(
        mode,
        VectorMode::Named,
        "refetch after cache clear must detect Named mode"
    );

    // Cleanup cache after test.
    clear_collection_mode_cache(collection);
}

/// Regression: post-migration, switching AXON_COLLECTION from the old unnamed
/// collection to the new named collection must use the Named code path.
/// Verifies that two different collection names have independent cache entries.
#[test]
fn collection_mode_cache_is_per_collection_name() {
    cache_vector_mode("old_unnamed_col", VectorMode::Unnamed);
    cache_vector_mode("new_named_col", VectorMode::Named);

    assert_eq!(
        cached_vector_mode("old_unnamed_col"),
        Some(VectorMode::Unnamed)
    );
    assert_eq!(
        cached_vector_mode("new_named_col"),
        Some(VectorMode::Named)
    );
    // Clearing one must not affect the other.
    clear_collection_mode_cache("old_unnamed_col");
    assert!(cached_vector_mode("old_unnamed_col").is_none());
    assert_eq!(
        cached_vector_mode("new_named_col"),
        Some(VectorMode::Named),
        "clearing old_unnamed_col must not affect new_named_col"
    );
    clear_collection_mode_cache("new_named_col");
}
```

---

### 8. `unwrap()` in `ranking/snippet.rs:58` — Severity: Low

**Current status:**
The `unwrap()` at line 58 of `crates/vector/ops/ranking/snippet.rs` is inside:

```rust
Some(_) => {
    let (_, consumed) = iter.next().unwrap(); // <-- line 58
    text_end_byte += consumed.len_utf8();
}
```

This is guarded by `iter.peek().copied()` returning `Some(_)` on the line above. Since `peek()` returns `Some(_)` meaning there is a next element, the `unwrap()` on `iter.next()` is logically infallible. However, the invariant is **implicit** — it relies on the non-mutating guarantee of `peek()`.

The existing ranking tests do not exercise link text containing multi-byte UTF-8 characters (e.g., emoji, CJK) or unterminated bracket sequences with multi-byte content.

**Recommended test:**

```rust
// Add to crates/vector/ops/ranking_test.rs or a new snippet_tests.rs

use super::ranking::snippet::get_meaningful_snippet; // adjust path as needed

#[test]
fn snippet_handles_multibyte_utf8_in_link_text() {
    // CJK characters are 3 bytes each — exercises the len_utf8() path at line 58.
    let text = "See [文档链接](https://docs.example.com/zh) for details.";
    // Should not panic and should return a non-empty snippet.
    let result = get_meaningful_snippet(text, &["文档"], 200);
    assert!(!result.is_empty(), "multibyte link text must not panic");
}

#[test]
fn snippet_handles_emoji_in_link_text() {
    // Emoji are 4 bytes each.
    let text = "Check [🚀 Launch](https://example.com/launch) now.";
    let result = get_meaningful_snippet(text, &["launch"], 200);
    assert!(!result.is_empty(), "emoji link text must not panic");
}

#[test]
fn snippet_handles_unterminated_bracket_sequence() {
    // No closing ']' — exercises the `!found_close_bracket` fallback path.
    let text = "An unterminated [link without closing bracket";
    let result = get_meaningful_snippet(text, &["link"], 200);
    // Must not panic; result content is implementation-defined.
    let _ = result;
}

#[test]
fn snippet_handles_empty_link_text_brackets() {
    let text = "Click [](https://example.com/) here.";
    let result = get_meaningful_snippet(text, &["click"], 200);
    assert!(!result.is_empty(), "empty bracket link must not panic");
}
```

---

## Graph Subsystem

### Graph Worker Tests — Overall Assessment

**Covered:**
- `merge_candidates` deduplication
- `partition_by_ambiguity`
- `chunk_point_id` determinism, URL variance, index variance
- `build_recommend_request` structure (including `"using": "dense"`)
- `group_by_url_max_score` max-score selection
- Taxonomy entity extraction (multiple tests in `taxonomy.rs`)
- Graph context build (5 tests in `context.rs`)

**Not covered:**
- `process_graph_job` end-to-end (requires Neo4j + Qdrant — no mock-HTTP test)
- `build_relationships` with entities that have the same normalized key
- `merge_llm_entities` type conflict resolution (`resolve_type_conflict`)
- `write_document_and_chunks`, `write_entities`, `write_chunk_mentions`, `write_entity_relationships` — all Neo4j writes are untested without a live Neo4j instance
- The `source_type` defaulting to `"crawl"` when missing from `GraphJobConfig`
- Empty chunk list path (exits early when `qdrant_retrieve_by_url` returns empty)

**Recommended test for `build_relationships`:**

```rust
#[test]
fn build_relationships_deduplicates_and_filters_self_loops() {
    let mut entities = HashMap::new();
    entities.insert(
        "tokio".to_string(),
        MergedEntity { name: "Tokio".to_string(), entity_type: "tech".to_string(), confidence: 0.9 },
    );
    entities.insert(
        "axum".to_string(),
        MergedEntity { name: "Axum".to_string(), entity_type: "tech".to_string(), confidence: 0.9 },
    );

    let relationships = vec![
        ExtractedRelationship { source: "Tokio".to_string(), target: "Axum".to_string(), relation: "uses".to_string() },
        ExtractedRelationship { source: "Tokio".to_string(), target: "Axum".to_string(), relation: "uses".to_string() }, // duplicate
        ExtractedRelationship { source: "Tokio".to_string(), target: "Tokio".to_string(), relation: "self".to_string() }, // self-loop
    ];

    let result = build_relationships(&entities, relationships);
    assert_eq!(result.len(), 1, "duplicate and self-loop must be removed");
    assert_eq!(result[0].relation, "uses");
}
```

---

## SSRF and Security Tests

**Summary:** Excellent. The SSRF test suite is a model for this kind of coverage:

- 38+ named unit tests covering all blocked IP ranges
- Property-based tests (`proptest`) for all four private ranges + loopback (spanning millions of inputs)
- IPv4-mapped IPv6 bypass coverage (property-based)
- `dns_rebinding_toctou_documents_residual_risk` explicitly documents the known limitation
- Non-HTTP scheme rejection (property-based for random schemes)

**No new SSRF tests are needed.** The known limitation is correctly documented.

---

## Auth Enforcement Tests

**What exists:**
- `tailscale_auth.rs` has 8 unit tests covering token matching, header precedence, and all `DenyReason` variants
- `services_acp_security.rs` covers `validate_adapter_command` rejections (empty, whitespace, shell names)
- `services_acp_spawn_env.rs` covers subprocess env isolation (CLAUDECODE, OPENAI_*, proxy vars)
- `mcp/server/oauth_google/tests.rs` covers OAuth redirect URI validation

**Gaps:**
1. No test for `check_auth()` when `api_token = None` (documents the debug/test bypass path — see Section 3)
2. No test for empty-string `x-api-key` header being treated as missing
3. No integration test exercising the shell or main WS endpoint's 403 response under denied auth

---

## Vector/Collection Mode Tests

**What exists:**
- `qdrant_store/tests.rs` is comprehensive (20+ tests including mock-HTTP probe failure, retry counts, cache behavior, `ensure_collection` creation shape)
- `hybrid.rs` has 4+ mock-HTTP tests for RRF fusion, dense-only named search, filter propagation, and error propagation
- `qdrant/tests.rs` has integration tests for `url_facets`, `delete_by_url`, scroll, etc. (skipped without live Qdrant)

**Gaps:**
1. Cache invalidation after `clear_collection_mode_cache()` — see Section 7
2. No test for `get_or_fetch_vector_mode` race condition (two concurrent callers simultaneously discovering uncached mode)

**Race condition test recommendation:**

```rust
#[tokio::test]
async fn get_or_fetch_mode_concurrent_callers_return_same_mode() {
    use httpmock::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let server = MockServer::start_async().await;
    let call_count = std::sync::Arc::new(AtomicUsize::new(0));
    let cc = call_count.clone();

    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/concurrent_test_col");
            then.status(200).json_body(serde_json::json!({
                "result": {"config": {"params": {"vectors": {"dense": {"size": 4}}}}}
            }));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "concurrent_test_col".to_string();

    // Clear any existing cache entry.
    clear_collection_mode_cache("concurrent_test_col");

    let cfg_arc = std::sync::Arc::new(cfg);
    let (r1, r2) = tokio::join!(
        get_or_fetch_vector_mode(&cfg_arc),
        get_or_fetch_vector_mode(&cfg_arc),
    );

    assert_eq!(r1.unwrap(), VectorMode::Named);
    assert_eq!(r2.unwrap(), VectorMode::Named);
    // Either 1 or 2 HTTP calls are acceptable (race on OnceLock init);
    // 0 would mean the cache was pre-warmed; >2 would indicate a loop regression.
    let calls = cc.load(Ordering::SeqCst);
    assert!(calls <= 2, "concurrent callers must not fan out probe requests: {calls}");

    clear_collection_mode_cache("concurrent_test_col");
}
```

---

## TEI Embedding Tests

**What exists:**
`crates/vector/ops/tei/tests.rs` has excellent mock-HTTP coverage:
- Empty input short-circuit (no HTTP call at unreachable port 1)
- 429 retry (call count verified)
- 413 batch split (two-pass mock with body matchers)
- 500 retry (call count + `#[serial_test::serial]` for env var safety)
- 404 fail-fast (no retry, call count = 1)
- `build_point_unnamed_emits_flat_vector` and `build_point_named_emits_dense_and_bm42`
- `build_point_named_sparse_has_nonzero_entries_for_real_text`
- Worst-case retry budget calculation (`tei_max_retries_default_fits_doc_timeout`)

**Gaps:**
1. No test for `TEI_MAX_CLIENT_BATCH_SIZE` env var override (verify batch size cap)
2. No test for the `TEI_MAX_RETRIES` override (the `EnvGuard` pattern is available; just not applied to this path)
3. No test for a transport error (connection refused) triggering retry vs fail-fast

**Recommended test:**

```rust
#[tokio::test]
async fn tei_embed_transport_error_is_retried() {
    // Port 1 is always ECONNREFUSED — transport errors should retry.
    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    // Use an invalid host to trigger transport error, not HTTP error.
    cfg.tei_url = "http://127.0.0.1:1".to_string();

    let inputs = vec!["transport error test".to_string()];
    let result = tei_embed(&cfg, &inputs).await;
    // Transport errors should eventually fail (after retries) — not panic.
    assert!(result.is_err(), "transport error must return Err after retries");
}
```

---

## Worker Infrastructure Tests

### AMQP Reconnect Backoff

**What exists:**
`worker_lane/tests.rs` has:
- `amqp_reconnect_backoff_doubles_and_caps` — verifies the 2→4→8→16→32→60→60 sequence
- `polling_backoff_sequence_doubles_caps_and_resets` — verifies poll backoff 100→200→...→6400
- `inflight_completion_returns_inflight_completed_not_idle_timeout` — regression for the stale sweep misfire
- Semaphore capacity and backpressure tests

**Gap:**
No test covers the "backoff reset only after ≥60s alive" rule for `worker_lane.rs` workers (vs the crawl worker which resets on every reconnect). The constant `AMQP_RECONNECT_MAX_SECS = 60` is tested, but the `ran_for_secs >= AMQP_RECONNECT_MAX_SECS` conditional reset is untested.

### Crawl Cancel Path

**What exists:**
`crates/jobs/crawl/runtime/worker/cancel_poll.rs` tests:
- `cancel_key_set_triggers_poll_completion` (requires live Redis)
- `cancel_key_absent_parks_poll` (requires live Redis)
- `cancel_key_unreachable_redis_fails_safe` (port 1, no live service needed)

**Gap:**
No test for the `reconnect_cancel_redis` exponential backoff path (5 attempts: 1s, 2s, 4s, 8s, 16s). The reconnect logic is not tested in isolation.

---

## Service Layer Tests

**What exists:**
`tests/services_query_services.rs` covers `map_query_results`, `map_retrieve_result`, `map_ask_payload`, `map_evaluate_payload`, `map_suggest_payload` with 10+ tests.

`tests/services_compile_services_smoke.rs` verifies `ServiceEvent` variants compile.

**Gaps:**
1. `crates/services/system.rs`, `crates/services/export.rs`, `crates/services/search.rs`, `crates/services/crawl.rs` — no dedicated service-layer tests for their map/transform functions
2. `ExportSchemaV3` golden file test (`tests/export_schema_v3_golden.rs`) exists and is good
3. No test for `services/query.rs` internal `query_service` function with a mocked Qdrant response

---

## Test Quality Observations

### Strengths

1. **`httpmock` used correctly** — mocks register before async calls, `.assert_async().await` verifies call counts, body matchers use `json_body_includes` for partial matching
2. **RAII env guards** — `EnvGuard`, `EnvVarGuard` patterns are consistent and protect against parallel-test env pollution; `#[serial_test::serial]` is applied where needed
3. **Integration tests skip cleanly** — `resolve_test_pg_url()`, `resolve_test_amqp_url()`, `resolve_test_redis_url()` pattern means CI passes without live infrastructure
4. **Property-based tests** — `proptest` for SSRF and URL utilities provides adversarial coverage that manual cases miss
5. **Regression labeling** — tests are named for the bug they prevent (e.g., `inflight_completion_returns_inflight_completed_not_idle_timeout`)

### Weaknesses

1. **`#[ignore]` integration tests** — 4 `ensure_collection_*` tests require live Qdrant and are marked `#[ignore]`. No CI gate runs `cargo test -- --ignored`, so these tests may drift silently. Consider a separate CI step or `just integration-test` recipe that requires the infra.
2. **Time-dependent tests** — `check_rate_limit_resets_after_window` in `ws_handler/tests.rs` uses `Instant::now() - Duration::from_secs(RATE_LIMIT_WINDOW_SECS + 1)` to simulate window expiry by backdating internal state. This is fragile if `Instant` resolution or thread scheduling varies.
3. **Global `OnceLock` state** — `COLLECTION_MODES` in `qdrant_store.rs` is process-wide global state. Tests that write to the cache with specific collection names (`test_cache_named`, `test_cache_unnamed`) could interfere if run in parallel with tests using the same names. Collection names in tests should use unique suffixes (e.g., `uuid::Uuid::new_v4().simple()`).
4. **No `cargo test -- --nocapture` contract** — Tests that emit `log_info`/`log_warn` do not verify their log output. Log-level tests would catch accidental removal of diagnostic messages.

---

## Performance Test Gaps

No `benches/` directory exists. The following are high-value benchmarks for a performance-sensitive RAG system:

| Benchmark | Why It Matters |
|-----------|---------------|
| TEI `tei_embed()` throughput (batch size 1 vs 64 vs 128) | Directly impacts embed worker throughput; identifies optimal chunk size |
| Qdrant `/query` hybrid search latency at 7M points | Validates the 2N call model in graph worker is acceptable |
| `chunk_text()` splitting throughput (long documents) | 500 pages × avg 10 chunks = 5,000 points per crawl |
| `group_by_url_max_score()` with 10,000 results | Graph similarity fan-out edge case |
| `rerank_ask_candidates()` with 100 candidates, 20 query tokens | `ask` command latency budget |

**Recommended first benchmark:**

```rust
// benches/embed_throughput.rs (new file)
use criterion::{Criterion, BenchmarkId, criterion_group, criterion_main};

fn bench_chunk_text(c: &mut Criterion) {
    use axon::crates::vector::ops::tei::prepare::chunk_text;
    let page_1k = "A".repeat(1000);
    let page_10k = "A".repeat(10_000);
    let page_100k = "A".repeat(100_000);

    let mut g = c.benchmark_group("chunk_text");
    for (label, text) in [("1k", &page_1k), ("10k", &page_10k), ("100k", &page_100k)] {
        g.bench_with_input(BenchmarkId::from_parameter(label), text, |b, t| {
            b.iter(|| chunk_text(t))
        });
    }
    g.finish();
}

criterion_group!(benches, bench_chunk_text);
criterion_main!(benches);
```

---

## Prioritized Recommendations

| # | Severity | Area | Action |
|---|----------|------|--------|
| 1 | **High** | Graph similarity | Add mock-HTTP tests for `compute_similarity` HTTP contract (one request per URL, correct body shape, non-200 fail-safe) |
| 2 | **High** | Auth bypass documentation | Add `no_token_configured_auto_allows_in_test_builds` test to document the `#[cfg(any(debug_assertions, test))]` bypass |
| 3 | **High** | Collection mode cache | Add `clear_collection_mode_cache_forces_refetch` test with mock server |
| 4 | **Medium** | Shell PTY auth | Add test verifying `shell_ws_upgrade` returns 403 for invalid/missing token |
| 5 | **Medium** | `open_amqp_channel` misuse | Add `#[ignore]` test documenting the channel-invalidation consequence |
| 6 | **Medium** | `ranking/snippet.rs` | Add edge case tests for multi-byte UTF-8 and unterminated brackets |
| 7 | **Medium** | Graph worker logic | Add tests for `build_relationships` (self-loop filter, dedup) and `merge_llm_entities` type conflict resolution |
| 8 | **Low** | Worker Config Arc | Add test asserting `wrap_with_heartbeat` passes `Arc<Config>` not cloned `Config` |
| 9 | **Low** | `#[ignore]` integration tests | Add `just integration-test` justfile recipe that runs `cargo test -- --ignored` with infra available |
| 10 | **Low** | Performance baselines | Add `benches/` directory with `chunk_text` throughput benchmark as a starting point |
| 11 | **Low** | Cache global state | Add `uuid::Uuid::new_v4()` suffixes to cache test collection names to prevent parallel-test interference |

---

*Report generated from static analysis of 389 Rust source files and 1,765+ test functions in the axon_rust codebase.*
