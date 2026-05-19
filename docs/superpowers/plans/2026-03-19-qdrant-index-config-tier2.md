# Qdrant Index Configuration (Tier 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add HNSW graph config, INT8 scalar quantization, and explicit `search_params` to all Qdrant search paths — improving memory efficiency by ~4x and giving explicit control over the recall/speed tradeoff.

**Architecture:** Fixes 5 and 6 extend the `create` JSON literal inside `ensure_collection()` (collection-creation-time-only). Fix 7 adds a `params` block to the three search functions (`qdrant_hybrid_search`, `qdrant_named_dense_search`, `qdrant_search`), reading `AXON_HNSW_EF_SEARCH` / `AXON_HNSW_EF_SEARCH_LEGACY` env vars via the existing `env_usize_clamped` helper. Because `client.rs` is already at its 500-line monolith limit, `qdrant_search()` is extracted to a new `search.rs` file before Fix 7 touches it.

**Tech Stack:** Rust, tokio, reqwest, serde_json, httpmock (for unit tests), Qdrant REST API

---

## File Map

| File | Status | Responsibility |
|------|--------|---------------|
| `crates/vector/ops/tei/qdrant_store.rs` | Modify | Fix 5 + 6: extend `create` JSON literal in `ensure_collection()` |
| `crates/vector/ops/tei/qdrant_store/tests.rs` | Modify | Tests for Fix 5 + 6 (httpmock, no live Qdrant) |
| `crates/vector/ops/qdrant/search.rs` | **Create** | Extracted `qdrant_search()` from `client.rs` + Fix 7 params |
| `crates/vector/ops/qdrant/client.rs` | Modify | Remove `qdrant_search()` body (delegated to `search.rs`); stays at or below 500 lines |
| `crates/vector/ops/qdrant/hybrid.rs` | Modify | Fix 7: add `params` block to `qdrant_hybrid_search()` and `qdrant_named_dense_search()` |
| `crates/vector/ops/qdrant.rs` | Modify | Add `mod search;` declaration + re-export `qdrant_search` from `search` mod |

---

## Deployment Note

**Fixes 5 and 6 are collection-creation-time-only.** They have zero effect on the existing `cortex` collection (which uses Unnamed mode — the early-return path in `ensure_collection()` fires before the `create` literal is reached). They apply automatically when:
- `cortex_v2` is created via `axon migrate --from cortex --to cortex_v2`
- Any future new collection is created

**Fix 7 applies immediately** to every search call on every collection the moment the binary is redeployed. No migration required.

**Recommended deployment sequence after merging this branch:**
1. Deploy the updated binary (all three fixes)
2. `axon migrate --from cortex --to cortex_v2` (creates `cortex_v2` with HNSW + quantization)
3. Set `AXON_COLLECTION=cortex_v2` in `.env`
4. Restart all workers

---

## Task 1: Fixes 5 + 6 — HNSW config and INT8 quantization in `ensure_collection()`

Both fixes modify the same JSON literal in `ensure_collection()`. Implementing them together avoids touching the literal twice and any associated merge complexity.

**Files:**
- Modify: `crates/vector/ops/tei/qdrant_store.rs` (line 328 — the `create` JSON literal)
- Modify: `crates/vector/ops/tei/qdrant_store/tests.rs`

### Background

`ensure_collection()` lives at line 280 of `qdrant_store.rs`. It:
1. Does a GET on the collection URL
2. If the collection already exists (2xx), returns early — **no creation JSON is ever built**
3. If 404, builds the `create` JSON and PUTs it

This means the `hnsw_config` and `quantization_config` additions below are unreachable for any existing collection. They only fire on first creation.

The file is currently 430 lines — the additions will bring it to ~445. Still within the 500-line monolith limit.

---

- [ ] **Step 1.1: Write the failing test for HNSW config**

Open `crates/vector/ops/tei/qdrant_store/tests.rs` and add these two tests at the bottom of the file (after the existing `get_or_fetch_mode_500_is_not_cached` test):

```rust
// -- Fix 5: ensure_collection sends hnsw_config on create --

#[tokio::test]
async fn ensure_collection_sends_hnsw_config_on_create() {
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    // GET → 404 (collection does not exist) triggers the creation path.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/hnsw_test_col");
            then.status(404);
        })
        .await;

    // PUT → expect hnsw_config in the body; respond 200 (success).
    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/hnsw_test_col")
                .json_body_includes(r#""hnsw_config":{"m":32,"ef_construct":256}"#);
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    // PUT → payload indexes (idempotent, any body accepted).
    server
        .mock_async(|when, then| {
            when.method(PUT).path_matches(
                regex::Regex::new("/collections/hnsw_test_col/index").unwrap(),
            );
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "hnsw_test_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(result.is_ok(), "ensure_collection must succeed: {:?}", result.err());
}

#[tokio::test]
async fn ensure_collection_does_not_put_on_existing_named_collection() {
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    // GET → 200 with a Named-mode collection body — triggers the early-return path.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/existing_named_col");
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "config": {
                        "params": {
                            "vectors": {
                                "dense": {"size": 4, "distance": "Cosine"}
                            },
                            "sparse_vectors": {
                                "bm42": {"modifier": "idf"}
                            }
                        }
                    }
                }
            }));
        })
        .await;

    // PUT → payload indexes (idempotent). Accept any body.
    server
        .mock_async(|when, then| {
            when.method(PUT).path_matches(
                regex::Regex::new("/collections/existing_named_col/index").unwrap(),
            );
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    // Explicitly reject any PUT to the collection URL itself (creation must NOT fire).
    let unexpected_create = server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/existing_named_col");
            then.status(200);
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "existing_named_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    assert!(result.is_ok(), "must succeed on existing collection: {:?}", result.err());
    assert_eq!(
        unexpected_create.hits_async().await,
        0,
        "collection PUT must NOT be called for an existing Named collection"
    );
}
```

- [ ] **Step 1.2: Run the failing tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -p axon -- ensure_collection_sends_hnsw_config_on_create ensure_collection_does_not_put_on_existing_named_collection 2>&1 | tail -30
```

Expected: `ensure_collection_sends_hnsw_config_on_create` FAILS because the PUT body doesn't yet contain `hnsw_config`. `ensure_collection_does_not_put_on_existing_named_collection` should already PASS (it tests existing behavior).

- [ ] **Step 1.3: Write the failing test for quantization config**

Add these two tests immediately after the HNSW tests in `crates/vector/ops/tei/qdrant_store/tests.rs`:

```rust
// -- Fix 6: ensure_collection sends quantization_config on create --

#[tokio::test]
async fn ensure_collection_sends_quantization_config_on_create() {
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/quant_test_col");
            then.status(404);
        })
        .await;

    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/quant_test_col")
                .json_body_includes(r#""quantization_config":{"scalar":{"type":"int8""#);
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    server
        .mock_async(|when, then| {
            when.method(PUT).path_matches(
                regex::Regex::new("/collections/quant_test_col/index").unwrap(),
            );
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "quant_test_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(result.is_ok(), "ensure_collection must succeed: {:?}", result.err());
}

#[tokio::test]
async fn ensure_collection_sends_full_create_body_with_hnsw_and_quantization() {
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/full_body_col");
            then.status(404);
        })
        .await;

    // Assert that both hnsw_config AND quantization_config appear in the same PUT body.
    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/full_body_col")
                .json_body_includes(r#""hnsw_config":{"m":32,"ef_construct":256}"#)
                .json_body_includes(r#""quantization_config":{"scalar":{"type":"int8""#);
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    server
        .mock_async(|when, then| {
            when.method(PUT).path_matches(
                regex::Regex::new("/collections/full_body_col/index").unwrap(),
            );
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "full_body_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(result.is_ok(), "full body test must succeed: {:?}", result.err());
}
```

- [ ] **Step 1.4: Run all four failing tests**

```bash
cargo test -p axon -- ensure_collection_sends_hnsw ensure_collection_sends_quantization ensure_collection_sends_full_create 2>&1 | tail -30
```

Expected: Three tests FAIL (hnsw, quantization, full body). The existing-collection test should still PASS.

- [ ] **Step 1.5: Implement Fixes 5 and 6 — extend the `create` JSON literal**

Open `crates/vector/ops/tei/qdrant_store.rs`. Find the `create` JSON literal at around line 328 (inside `ensure_collection()`). Replace it with:

```rust
    let create = serde_json::json!({
        "vectors": {
            "dense": {"size": dim, "distance": "Cosine"}
        },
        "sparse_vectors": {
            "bm42": {"modifier": "idf"}
        },
        "hnsw_config": {
            "m": 32,
            "ef_construct": 256
        },
        "quantization_config": {
            "scalar": {
                "type": "int8",
                "quantile": 0.99,
                "always_ram": true
            }
        }
    });
```

Nothing else in `ensure_collection()` changes. The `validate_existing_dim`, `patch_add_sparse`, and `ensure_payload_indexes` paths are entirely unaffected.

- [ ] **Step 1.6: Run the full test suite for Task 1**

```bash
cargo test -p axon -- ensure_collection 2>&1 | tail -40
```

Expected: ALL `ensure_collection_*` tests PASS (including the four new ones and the three existing `#[ignore]` integration tests, which won't run unless you pass `--ignored`). Zero failures.

- [ ] **Step 1.7: Lint and format check**

```bash
cargo clippy -p axon 2>&1 | grep -E "^error|warning\[" | head -20
cargo fmt --check 2>&1
```

Expected: No errors. Fix any warnings before proceeding.

- [ ] **Step 1.8: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add crates/vector/ops/tei/qdrant_store.rs crates/vector/ops/tei/qdrant_store/tests.rs
git commit -m "feat(vector): add HNSW config (m=32, ef_construct=256) and INT8 quantization to ensure_collection()

New Named collections (including cortex_v2 via axon migrate) will be created with:
- hnsw_config: m=32, ef_construct=256 (better graph connectivity at 7M+ points)
- quantization_config: int8 scalar, quantile=0.99, always_ram=true (~4x storage reduction)

Existing Unnamed collections (cortex) are unaffected — ensure_collection() returns
early on existing collections before the create literal is reached."
```

---

## Task 2: Fix 7a — Extract `qdrant_search()` to `search.rs` (monolith budget relief)

`client.rs` is at 520 lines — 20 lines over the hard limit. Before Fix 7 can add tests for the legacy search path, `qdrant_search()` must be extracted to its own file.

**Files:**
- Create: `crates/vector/ops/qdrant/search.rs`
- Modify: `crates/vector/ops/qdrant/client.rs` (remove `qdrant_search` body)
- Modify: `crates/vector/ops/qdrant.rs` (add `mod search;` + re-export)

---

- [ ] **Step 2.1: Verify the current state of `client.rs` and `qdrant.rs`**

```bash
wc -l /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/client.rs
grep -n "qdrant_search" /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/client.rs
grep -n "qdrant_search" /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant.rs
```

Expected: `client.rs` ~520 lines; `qdrant_search` function body at ~line 427; `qdrant.rs` exports `qdrant_search` from `client`.

- [ ] **Step 2.2: Create `crates/vector/ops/qdrant/search.rs`**

This file receives the `qdrant_search()` function body cut from `client.rs`. It needs the same imports used by that function.

```rust
//! Legacy dense-only search for Unnamed collections.
//!
//! Unnamed collections (created before named-vector support) use `/points/search`
//! with a flat `"vector"` field. Named collections use `/points/query` via
//! [`hybrid`](super::hybrid).

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use anyhow::{Result, anyhow};
use std::time::Instant;

use super::types::{QdrantSearchHit, QdrantSearchResponse};
use super::utils::{env_usize_clamped, qdrant_base};

/// Dense-only vector search for Unnamed (legacy) collections.
///
/// Issues a POST to `/collections/{name}/points/search` with a flat `"vector"` field.
/// Named collections must use [`qdrant_hybrid_search`](super::hybrid::qdrant_hybrid_search)
/// or [`qdrant_named_dense_search`](super::hybrid::qdrant_named_dense_search) instead.
///
/// `hnsw_ef` is read from `AXON_HNSW_EF_SEARCH_LEGACY` (default 64, clamped [32, 512]).
/// The `quantization.rescore` field in `params` is harmless for collections without
/// quantization configured — Qdrant ignores it silently.
pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client().map_err(|e| anyhow!(e.to_string()))?;
    let url = format!(
        "{}/collections/{}/points/search",
        qdrant_base(cfg),
        cfg.collection
    );
    let hnsw_ef = env_usize_clamped("AXON_HNSW_EF_SEARCH_LEGACY", 64, 32, 512);
    let search_start = Instant::now();
    let mut body = serde_json::json!({
        "vector": vector,
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
        "params": {
            "hnsw_ef": hnsw_ef,
            "quantization": {
                "rescore": true,
                "oversampling": 1.5
            }
        }
    });
    if let Some(f) = filter {
        body["filter"] = f.clone();
    }
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?
        .json::<QdrantSearchResponse>()
        .await?;
    log_debug(&format!(
        "qdrant search hits={} collection={}",
        res.result.len(),
        cfg.collection
    ));
    Ok(res.result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
        let result: Vec<serde_json::Value> = hits
            .iter()
            .map(|(url, score)| {
                serde_json::json!({
                    "id": "test-id",
                    "score": score,
                    "payload": {"url": url, "chunk_text": "test chunk"}
                })
            })
            .collect();
        serde_json::json!({"result": result})
    }

    #[tokio::test]
    async fn qdrant_search_sends_hnsw_ef_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#""params""#)
                    .json_body_includes(r#""hnsw_ef""#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/legacy", 0.85)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "qdrant_search must succeed: {:?}", result.err());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn qdrant_search_sends_quantization_rescore_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#""rescore":true"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/x", 0.77)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "rescore param test must succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn qdrant_search_propagates_http_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/collections/test_col/points/search");
                then.status(500).body("internal error");
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32], 5, None).await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }
}
```

- [ ] **Step 2.3: Remove `qdrant_search()` from `client.rs`**

Open `crates/vector/ops/qdrant/client.rs`. Find the `qdrant_search` function (starting at ~line 427). Delete the entire function body (from `pub(crate) async fn qdrant_search` through its closing `}`). The file should drop below 480 lines.

Also remove the `anyhow` import line at the top if it is now unused — check with `cargo check`.

> **Note:** The `upsert_and_search_roundtrip` integration test in `crates/vector/ops/qdrant/tests.rs` imports `qdrant_search` from `client`. You must update that import to come from `search` after the move. Open `crates/vector/ops/qdrant/tests.rs` and change:
> ```rust
> use super::client::{
>     qdrant_delete_by_url_filter, qdrant_delete_stale_domain_urls, qdrant_domain_facets,
>     qdrant_retrieve_by_url, qdrant_scroll_pages, qdrant_search, qdrant_url_facets,
> };
> ```
> to:
> ```rust
> use super::client::{
>     qdrant_delete_by_url_filter, qdrant_delete_stale_domain_urls, qdrant_domain_facets,
>     qdrant_retrieve_by_url, qdrant_scroll_pages, qdrant_url_facets,
> };
> use super::search::qdrant_search;
> ```

- [ ] **Step 2.4: Register `search.rs` as a module and redirect the `qdrant_search` re-export**

Open `crates/vector/ops/qdrant.rs`. Make exactly **two targeted changes** — add `mod search;` and redirect `qdrant_search` from `client` to `search`. Do **not** change any other re-exports; the existing pub/pub(crate) exports are correct and must not be altered by this refactor.

**Change 1** — Add `mod search;` after `mod hybrid;`:

Old:
```rust
mod hybrid;
#[cfg(test)]
mod tests;
```

New:
```rust
mod hybrid;
mod search;   // ← ADD THIS LINE
#[cfg(test)]
mod tests;
```

**Change 2** — In the `pub(crate) use client::{}` block, remove `qdrant_search` and add a new `pub(crate) use search::qdrant_search;` line after the block:

Old:
```rust
pub(crate) use client::{
    qdrant_delete_stale_tail, qdrant_domain_facets, qdrant_retrieve_by_url,
    qdrant_scroll_pages_while, qdrant_search,
};
```

New:
```rust
pub(crate) use client::{
    qdrant_delete_stale_tail, qdrant_domain_facets, qdrant_retrieve_by_url,
    qdrant_scroll_pages_while,
};
pub(crate) use search::qdrant_search;   // ← moved from client
```

> **Important:** Do NOT add `qdrant_url_facets` or `dispatch_vector_search` to the re-exports — those are not part of this refactor. The existing file has exactly the re-exports it needs; only `qdrant_search`'s source module changes.

- [ ] **Step 2.5: Verify compilation and run existing qdrant tests**

```bash
cargo check -p axon 2>&1 | grep "^error" | head -20
cargo test -p axon -- qdrant_search 2>&1 | tail -20
```

Expected: Zero compile errors. The three new tests in `search.rs` (`qdrant_search_sends_hnsw_ef_param`, `qdrant_search_sends_quantization_rescore_param`, `qdrant_search_propagates_http_error`) all PASS immediately — the implementation in Step 2.2 already includes the `params` block.

If any existing tests in `qdrant/tests.rs` fail after this step, it is because of the import change in Step 2.3 — fix that import.

- [ ] **Step 2.6: Confirm line counts**

```bash
wc -l /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/client.rs
wc -l /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/search.rs
```

Expected: `client.rs` < 500 lines. `search.rs` < 120 lines.

- [ ] **Step 2.7: Lint and format**

```bash
cargo clippy -p axon 2>&1 | grep -E "^error|warning\[" | head -20
cargo fmt --check 2>&1
```

Fix any issues. Run `cargo fmt` if needed, then re-check.

- [ ] **Step 2.8: Commit**

```bash
git add \
  crates/vector/ops/qdrant/search.rs \
  crates/vector/ops/qdrant/client.rs \
  crates/vector/ops/qdrant/tests.rs \
  crates/vector/ops/qdrant.rs
git commit -m "refactor(vector): extract qdrant_search() to search.rs to restore monolith budget

client.rs was at 520 lines (max 500). qdrant_search() moved to search.rs with
its own tests. Re-exported via qdrant.rs; all callsites and integration tests updated."
```

---

## Task 3: Fix 7b — Add `search_params` to `qdrant_hybrid_search()` and `qdrant_named_dense_search()`

**Files:**
- Modify: `crates/vector/ops/qdrant/hybrid.rs`

Both Named-mode search functions in `hybrid.rs` need a `params` block added to their JSON body. The env var is `AXON_HNSW_EF_SEARCH` (default 128, range [32, 512]).

`env_usize_clamped` is accessible in `hybrid.rs` as `super::utils::env_usize_clamped`.

---

- [ ] **Step 3.1: Write the failing tests**

Open `crates/vector/ops/qdrant/hybrid.rs`. Find the `#[cfg(test)] mod tests` block at the bottom of the file. Add these tests after the existing `qdrant_named_dense_search_includes_filter_when_some` test:

```rust
    #[tokio::test]
    async fn qdrant_hybrid_search_sends_hnsw_ef_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#""params""#)
                    .json_body_includes(r#""hnsw_ef""#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/h", 0.9)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid ef test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "hybrid hnsw_ef test must succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_sends_quantization_rescore_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#""rescore":true"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/q", 0.88)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid rescore test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "hybrid rescore test must succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_sends_hnsw_ef_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#""params""#)
                    .json_body_includes(r#""hnsw_ef""#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/nd", 0.77)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let result = qdrant_named_dense_search(&cfg, &dense, 5, None).await;

        mock.assert_async().await;
        assert!(result.is_ok(), "named dense hnsw_ef test must succeed: {:?}", result.err());
    }
```

- [ ] **Step 3.2: Run the failing tests**

```bash
cargo test -p axon -- qdrant_hybrid_search_sends_hnsw qdrant_hybrid_search_sends_quantization qdrant_named_dense_search_sends_hnsw 2>&1 | tail -20
```

Expected: All three tests FAIL — the `params` key does not yet exist in the JSON bodies.

- [ ] **Step 3.3: Add `params` to `qdrant_hybrid_search()`**

Open `crates/vector/ops/qdrant/hybrid.rs`. Find `qdrant_hybrid_search()`. The function currently builds `body` as a `serde_json::json!` literal. Modify it to include the `params` block:

Add this near the top of the function, before the `body` construction:

```rust
    let hnsw_ef = super::utils::env_usize_clamped("AXON_HNSW_EF_SEARCH", 128, 32, 512);
```

Then change the `body` literal to include a `params` key. The full updated `body` definition:

```rust
    let mut body = serde_json::json!({
        "prefetch": [
            {
                "query": dense_vector,
                "using": "dense",
                "limit": candidates
            },
            {
                "query": sparse_vector.to_json(),
                "using": "bm42",
                "limit": candidates
            }
        ],
        "query": {"fusion": "rrf"},
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
        "params": {
            "hnsw_ef": hnsw_ef,
            "quantization": {
                "rescore": true,
                "oversampling": 1.5
            }
        }
    });
```

- [ ] **Step 3.4: Add `params` to `qdrant_named_dense_search()`**

In the same file, find `qdrant_named_dense_search()`. Add the env var read and the `params` block:

```rust
    let hnsw_ef = super::utils::env_usize_clamped("AXON_HNSW_EF_SEARCH", 128, 32, 512);
    let mut body = serde_json::json!({
        "query": dense_vector,
        "using": "dense",
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
        "params": {
            "hnsw_ef": hnsw_ef,
            "quantization": {
                "rescore": true,
                "oversampling": 1.5
            }
        }
    });
```

- [ ] **Step 3.5: Run the tests to confirm they pass**

```bash
cargo test -p axon -- qdrant_hybrid_search qdrant_named_dense_search 2>&1 | tail -30
```

Expected: ALL `qdrant_hybrid_search_*` and `qdrant_named_dense_search_*` tests pass, including the previously passing ones (`sends_prefetch_rrf_query`, `uses_query_endpoint_with_dense_using`, `propagates_error`, `includes_filter_when_some`).

- [ ] **Step 3.6: Check line count**

```bash
wc -l /home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/hybrid.rs
```

Expected: ≤ 390 lines (added ~50 lines). If it exceeds 500, the test helpers need to be extracted to the `tests` module — that would be unusual given the current 337-line starting point.

- [ ] **Step 3.7: Lint and format**

```bash
cargo clippy -p axon 2>&1 | grep -E "^error|warning\[" | head -20
cargo fmt --check 2>&1
```

- [ ] **Step 3.8: Commit**

```bash
git add crates/vector/ops/qdrant/hybrid.rs
git commit -m "feat(vector): add hnsw_ef and quantization rescore params to Named-mode search paths

Both qdrant_hybrid_search() and qdrant_named_dense_search() now send:
  params.hnsw_ef = AXON_HNSW_EF_SEARCH (default 128, range [32, 512])
  params.quantization.rescore = true
  params.quantization.oversampling = 1.5

This activates quantization rescoring (full-precision rerank of int8 candidates)
and gives explicit HNSW traversal depth control for Named-mode collections."
```

---

## Task 4: Full verification pass

- [ ] **Step 4.1: Run the complete test suite**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -p axon 2>&1 | tail -30
```

Expected: All tests pass. Zero failures. The count of passing tests should be higher than before this feature branch (new tests added in Tasks 1–3).

- [ ] **Step 4.2: Run `just verify`**

```bash
just verify
```

This runs `cargo fmt --check`, `cargo clippy`, `cargo check`, and `cargo test`. Expected: all pass.

If `just` is not available:

```bash
cargo fmt --check && cargo clippy -p axon -- -D warnings && cargo test -p axon
```

- [ ] **Step 4.3: Spot-check that `AXON_HNSW_EF_SEARCH_LEGACY` is documented in `.env.example`**

```bash
grep -n "AXON_HNSW" /home/jmagar/workspace/axon_rust/.env.example
```

If neither `AXON_HNSW_EF_SEARCH` nor `AXON_HNSW_EF_SEARCH_LEGACY` appears, add them:

```bash
# In .env.example, add in the "Worker tuning" section:
# AXON_HNSW_EF_SEARCH=128          # HNSW traversal depth for Named-mode search (default 128, range 32-512)
# AXON_HNSW_EF_SEARCH_LEGACY=64    # HNSW traversal depth for Unnamed (legacy dense-only) search (default 64)
```

Then: `git add .env.example`

- [ ] **Step 4.4: Verify CLAUDE.md env var table**

Open `CLAUDE.md` and find the "Key Env Vars" table under the `crates/vector` section. Add the two new env vars if not present:

| Var | Default | Effect |
|-----|---------|--------|
| `AXON_HNSW_EF_SEARCH` | 128 (range 32–512) | HNSW traversal depth for Named-mode search (hybrid + named dense) |
| `AXON_HNSW_EF_SEARCH_LEGACY` | 64 (range 32–512) | HNSW traversal depth for Unnamed (legacy dense-only) search |

This is in `crates/vector/CLAUDE.md`, not the root `CLAUDE.md`.

- [ ] **Step 4.5: Final commit**

```bash
git add .env.example crates/vector/CLAUDE.md
git commit -m "docs: add AXON_HNSW_EF_SEARCH and AXON_HNSW_EF_SEARCH_LEGACY to env.example and CLAUDE.md"
```

---

## Summary: What Changed and Why

| Fix | File(s) | What | Why |
|-----|---------|------|-----|
| Fix 5 | `qdrant_store.rs` | `hnsw_config: {m:32, ef_construct:256}` in `create` | M=16 default gives poor graph connectivity at 7M+ points; M=32 doubles graph edges for better recall |
| Fix 6 | `qdrant_store.rs` | `quantization_config: int8 scalar, always_ram` | 7M × 1024 FP32 ≈ 28 GB; INT8 reduces to ~7 GB; `always_ram` keeps quantized vecs hot for candidate filtering |
| Fix 7 | `hybrid.rs`, `search.rs` | `params: {hnsw_ef, quantization: {rescore, oversampling}}` | Without `rescore:true`, quantized search returns int8 scores; rescoring re-ranks top candidates with full FP32 vectors. `hnsw_ef` controls recall/latency tradeoff at query time |
| Refactor | `client.rs` → `search.rs` | Extracted `qdrant_search()` | `client.rs` was at 520 lines, over the 500-line monolith limit |

**Env vars introduced (no Config struct changes, no CLI flags):**

| Var | Default | Range | Used in |
|-----|---------|-------|---------|
| `AXON_HNSW_EF_SEARCH` | 128 | 32–512 | `qdrant_hybrid_search`, `qdrant_named_dense_search` |
| `AXON_HNSW_EF_SEARCH_LEGACY` | 64 | 32–512 | `qdrant_search` (Unnamed collections) |
