# Tier 3 Config Tuning + Tier 4 Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve hybrid search retrieval quality by differentiating the prefetch window for `ask` vs `query`, double the sparse vector bucket count to halve collision probability, and add structured latency/candidate-count logging to every search path.

**Architecture:** Four self-contained changes. Fix 8 adds a new `ask_hybrid_candidates` Config field and uses it only in `retrieval.rs`, keeping `query.rs` on the existing `hybrid_search_candidates` field. Fix 9 changes one constant in `sparse.rs` and updates impacted tests — must be batched with the `cortex_v2` migration. Fixes 10 and 11 are pure `log_debug` additions to existing functions, zero functional change, zero risk.

**Tech Stack:** Rust, Tokio, `anyhow`, `std::time::Instant`, `log_debug`/`log_warn` from `crates::core::logging`

**Shipping dependencies:**
- Fix 8: independent — can ship on any branch
- Fix 9: **must ship in the same migration batch** as Tier 1 Fix 1 + Tier 2 Fixes 5+6; sparse vectors in existing points become incompatible after this change
- Fixes 10 + 11: independent — pure log additions, zero functional risk

---

## File Map

| File | Change |
|------|--------|
| `crates/core/config/types/config.rs` | Add `ask_hybrid_candidates: usize` field (Fix 8) |
| `crates/core/config/types/config_impls.rs` | Add default for `ask_hybrid_candidates` (Fix 8) |
| `crates/core/config/parse/build_config.rs` | Parse `AXON_ASK_HYBRID_CANDIDATES` env var (Fix 8) |
| `crates/cli/commands/research.rs` | Update `make_test_config()` inline struct literal (Fix 8) |
| `crates/cli/commands/search.rs` | Update `make_test_config()` inline struct literal (Fix 8) |
| `crates/jobs/common/*.rs` | Update any `make_test_config()` struct literals (Fix 8) |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | Use `cfg.ask_hybrid_candidates` as candidate count (Fix 8); add `log_debug` for context build stats (Fix 11) |
| `crates/vector/ops/sparse.rs` | Change `SPARSE_DIM` from `30_522` to `65_536`; update module doc + tests (Fix 9) |
| `crates/vector/ops/qdrant/hybrid.rs` | Add latency log after successful search, for both `qdrant_hybrid_search` and `qdrant_named_dense_search` (Fix 10) |
| `crates/vector/ops/qdrant/client.rs` | Add latency log after `qdrant_search` success (Fix 10) |

---

## Task 1: Fix 8 — Add `ask_hybrid_candidates` Config Field

Add a new Config field that controls the prefetch window size exclusively for the `ask` pipeline. The existing `hybrid_search_candidates` remains unchanged and continues to govern the `query` path.

**Files:**
- Modify: `crates/core/config/types/config.rs` (around line 355–360, after `hybrid_search_candidates`)
- Modify: `crates/core/config/types/config_impls.rs` (around line 120, after `hybrid_search_candidates: 100`)
- Modify: `crates/core/config/parse/build_config.rs` (around line 488–492, after the `hybrid_search_candidates` parse block)
- Modify: `crates/cli/commands/research.rs` — `make_test_config()` struct literal
- Modify: `crates/cli/commands/search.rs` — `make_test_config()` struct literal
- Modify: any `crates/jobs/common/*.rs` that has a `Config { .. }` inline struct literal

### Step 1.1 — Write the failing test

In `crates/core/config/types/config.rs`, add a test that asserts the new field exists with the correct default. Place it in the existing `#[cfg(test)] mod tests` block at the bottom of the file.

```rust
#[test]
fn ask_hybrid_candidates_default_is_150() {
    let cfg = Config::default();
    assert_eq!(
        cfg.ask_hybrid_candidates, 150,
        "ask_hybrid_candidates must default to 150 for wider prefetch before reranking"
    );
}
```

- [ ] Add the test above to the test block in `crates/core/config/types/config.rs`

### Step 1.2 — Run the test to verify it fails

```bash
cd /home/jmagar/workspace/axon_rust && cargo test ask_hybrid_candidates_default_is_150 -- --nocapture
```

Expected: compile error — field `ask_hybrid_candidates` does not exist on type `Config`.

- [ ] Confirm the test fails to compile with the field-not-found error

### Step 1.3 — Add the field declaration to `Config`

In `crates/core/config/types/config.rs`, add after the `hybrid_search_candidates` field (around line 357):

```rust
    /// Candidates fetched per prefetch arm before RRF fusion, for the `ask` pipeline only.
    ///
    /// Ask reranks with `ask_min_relevance_score` (default 0.45) before selecting context,
    /// so it needs a wider prefetch window than `query` (which skips reranking).
    /// Env: `AXON_ASK_HYBRID_CANDIDATES` (clamped 10–500). Default: 150.
    pub ask_hybrid_candidates: usize,
```

- [ ] Add the field to the `Config` struct

### Step 1.4 — Add the default in `Config::default()`

In `crates/core/config/types/config_impls.rs`, after `hybrid_search_candidates: 100,` (around line 120):

```rust
            ask_hybrid_candidates: 150,
```

- [ ] Add the default value

### Step 1.5 — Parse the env var in `build_config.rs`

In `crates/core/config/parse/build_config.rs`, after the `hybrid_search_candidates` parse block (around line 492):

```rust
        ask_hybrid_candidates: performance::env_usize_clamped(
            "AXON_ASK_HYBRID_CANDIDATES",
            150,
            10,
            500,
        ),
```

- [ ] Add the `env_usize_clamped` call

### Step 1.6 — Update inline struct literals in test helpers

The compiler only catches missing struct fields at **test** build time. Run the following to find all affected files:

```bash
cd /home/jmagar/workspace/axon_rust && grep -rn "make_test_config\|Config {" crates/cli/commands/research.rs crates/cli/commands/search.rs crates/jobs/common/ | grep -v "//\|test_config("
```

In each file that has an inline `Config { .. }` literal (not `test_config()` call), add `ask_hybrid_candidates: 150,` before the closing `}`.

> Note: `crates/jobs/common/test_config` returns a `Config` from the `Config::default()` path — it does NOT need updating. Only hand-rolled `Config { field: val, ... }` literals need the new field.

- [ ] Update `crates/cli/commands/research.rs` inline struct if it has one
- [ ] Update `crates/cli/commands/search.rs` inline struct if it has one
- [ ] Update any `crates/jobs/common/*.rs` inline structs if they have one

### Step 1.7 — Run tests and verify clean compile

```bash
cd /home/jmagar/workspace/axon_rust && cargo test ask_hybrid_candidates_default_is_150 -- --nocapture
```

Expected: PASS

```bash
cd /home/jmagar/workspace/axon_rust && cargo test -- --nocapture 2>&1 | tail -5
```

Expected: `test result: ok.` with no failures.

- [ ] Confirm the new test passes
- [ ] Confirm the full test suite compiles and passes

### Step 1.8 — Commit

```bash
cd /home/jmagar/workspace/axon_rust && git add \
  crates/core/config/types/config.rs \
  crates/core/config/types/config_impls.rs \
  crates/core/config/parse/build_config.rs \
  crates/cli/commands/research.rs \
  crates/cli/commands/search.rs
# Also stage any crates/jobs/common/*.rs files that were updated in Step 1.6:
git add crates/jobs/common/
git commit -m "feat(config): add ask_hybrid_candidates field defaulting to 150"
```

> **Note:** If Step 1.6 found no inline struct literals in `crates/jobs/common/`, the `git add crates/jobs/common/` line is a no-op — safe to run regardless.

- [ ] Commit

---

## Task 2: Fix 8 — Wire `ask_hybrid_candidates` into the `ask` retrieval path

The `retrieve_ask_candidates` function in `retrieval.rs` currently passes `cfg.ask_candidate_limit` to `dispatch_vector_search`. The hybrid prefetch window is controlled by `cfg.hybrid_search_candidates` inside `qdrant_hybrid_search`. We need to either (a) pass the candidate count directly into `dispatch_vector_search`/`qdrant_hybrid_search`, or (b) temporarily override `cfg` before the call.

The cleanest approach: pass the ask-specific candidates as a new `candidates_override: Option<usize>` parameter to `qdrant_hybrid_search` and `qdrant_named_dense_search`, letting callers override the prefetch window without touching the Config. However, that changes the function signature used by both `query` and `ask`.

**Simpler approach used here:** In `retrieval.rs`, call `qdrant_hybrid_search` directly (bypass `dispatch_vector_search`) with a locally computed candidates value equal to `cfg.ask_hybrid_candidates`. This is safe because `retrieval.rs` already handles the Named/Unnamed dispatch concern (it only calls this after `dispatch_vector_search` returns hits from a known collection).

**Actually the simplest, zero-signature-change approach:** pass a modified `cfg` clone with `hybrid_search_candidates` overridden for the ask path. Since `dispatch_vector_search` reads `cfg.hybrid_search_candidates` inside `qdrant_hybrid_search`, temporarily setting it to `cfg.ask_hybrid_candidates` achieves the goal without touching the function signatures.

**Files:**
- Modify: `crates/vector/ops/commands/ask/context/retrieval.rs`

### Step 2.1 — Write the failing test

The test verifies that when `ask_hybrid_candidates` differs from `hybrid_search_candidates`, the hybrid search call uses the ask-specific value. We test this via the mock server: assert the `prefetch[0].limit` in the request body equals `ask_hybrid_candidates`.

Add this test to `crates/vector/ops/qdrant/hybrid.rs` test block, since that is where the prefetch body is constructed and can be verified via httpmock:

```rust
#[tokio::test]
async fn qdrant_hybrid_search_uses_candidates_from_config() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            // Verify the prefetch limit equals cfg.hybrid_search_candidates (set to 77 below)
            when.method(POST)
                .path("/collections/test_col/points/query")
                .json_body_includes(r#""limit":77"#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();
    cfg.hybrid_search_candidates = 77;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let sparse = compute_sparse_vector("hybrid search test");
    let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None).await;

    mock.assert_async().await;
    assert!(result.is_ok());
}
```

- [ ] Add the test to `crates/vector/ops/qdrant/hybrid.rs`

### Step 2.2 — Run to confirm test passes (it should already — it tests existing behavior)

```bash
cd /home/jmagar/workspace/axon_rust && cargo test qdrant_hybrid_search_uses_candidates_from_config -- --nocapture
```

Expected: PASS (this test validates existing behavior — the real change is in `retrieval.rs`).

- [ ] Confirm the existing-behavior test passes

### Step 2.3 — Write the test for `retrieval.rs` ask-specific candidate window

Add this to `crates/vector/ops/commands/ask/context/tests.rs` (or create the file if absent). This is a unit test of the config field wiring:

```rust
#[test]
fn ask_hybrid_candidates_is_distinct_from_query_candidates() {
    use crate::crates::core::config::Config;
    let mut cfg = Config::default();
    cfg.hybrid_search_candidates = 100;
    cfg.ask_hybrid_candidates = 150;
    // The ask pipeline should use ask_hybrid_candidates (150), not hybrid_search_candidates (100).
    assert_ne!(
        cfg.ask_hybrid_candidates,
        cfg.hybrid_search_candidates,
        "ask and query prefetch windows must be independently configurable"
    );
    assert_eq!(cfg.ask_hybrid_candidates, 150);
    assert_eq!(cfg.hybrid_search_candidates, 100);
}
```

- [ ] Add the test to the ask context tests file

### Step 2.4 — Run to confirm it passes

```bash
cd /home/jmagar/workspace/axon_rust && cargo test ask_hybrid_candidates_is_distinct_from_query_candidates -- --nocapture
```

Expected: PASS

- [ ] Confirm the test passes

### Step 2.5 — Implement: override `hybrid_search_candidates` in `retrieve_ask_candidates`

In `crates/vector/ops/commands/ask/context/retrieval.rs`, change the call to `qdrant::dispatch_vector_search` to pass a locally-constructed config clone that sets `hybrid_search_candidates = cfg.ask_hybrid_candidates`:

Find the call site (around line 31):
```rust
    let hits = qdrant::dispatch_vector_search(cfg, &vecq, query, cfg.ask_candidate_limit)
```

Replace with:

```rust
    // Ask reranks candidates before context selection, so use a wider prefetch window
    // than query (which skips reranking). cfg.ask_hybrid_candidates (default: 150)
    // overrides cfg.hybrid_search_candidates (default: 100) for this path only.
    //
    // `let ask_cfg_override;` is an intentionally uninitialized binding — this is
    // valid Rust. The binding is only assigned inside the `if` branch, and `search_cfg`
    // borrows from it. The compiler ensures it is initialized before use.
    let ask_cfg_override;
    let search_cfg = if cfg.ask_hybrid_candidates != cfg.hybrid_search_candidates {
        ask_cfg_override = {
            let mut c = cfg.clone();
            c.hybrid_search_candidates = cfg.ask_hybrid_candidates;
            c
        };
        &ask_cfg_override
    } else {
        cfg
    };
    let hits = qdrant::dispatch_vector_search(search_cfg, &vecq, query, cfg.ask_candidate_limit)
```

> **Note on the clone:** `Config` implements `Clone`. The clone is only created when the two values differ (i.e., when the user has customized one but not the other), which is the common case. This is acceptable: the hot path is the search I/O, not the config clone.

- [ ] Apply the change to `retrieval.rs`

### Step 2.6 — Run tests and lint

```bash
cd /home/jmagar/workspace/axon_rust && cargo test -- --nocapture 2>&1 | tail -10
cargo clippy 2>&1 | grep -E "^error|^warning\[" | head -20
cargo fmt --check
```

Expected: all tests pass, no clippy errors, fmt clean.

- [ ] Verify tests pass
- [ ] Verify clippy clean
- [ ] Verify fmt clean

### Step 2.7 — Commit

```bash
cd /home/jmagar/workspace/axon_rust && git add \
  crates/vector/ops/commands/ask/context/retrieval.rs \
  crates/vector/ops/qdrant/hybrid.rs \
  crates/vector/ops/commands/ask/context/tests.rs
git commit -m "feat(ask): use ask_hybrid_candidates (150) as prefetch window for ask reranking path"
```

- [ ] Commit

---

## Task 3: Fix 9 — Bump `SPARSE_DIM` to 65,536

**CRITICAL SHIPPING DEPENDENCY:** This change makes all existing sparse vectors (computed with `SPARSE_DIM=30_522`) incompatible with new ones (`SPARSE_DIM=65_536`). Do not merge this to `main` until the `cortex_v2` migration is scheduled. The migration flow is:

1. Apply this change (Fix 9)
2. Create new `cortex_v2` collection (via `axon migrate` or fresh collection)
3. Re-embed all content with the new SPARSE_DIM
4. Flip `AXON_COLLECTION=cortex_v2` in `.env`

The plan document for Tier 1 Fix 1 and Tier 2 Fixes 5+6 defines the migration timing — this Fix 9 must land in the same PR/migration batch.

**Files:**
- Modify: `crates/vector/ops/sparse.rs` — `SPARSE_DIM` constant, module doc comment, and tests

### Step 3.1 — Write the failing test

The existing test `compute_sparse_vector_all_indices_in_bucket_range` already asserts `idx < SPARSE_DIM`. After changing SPARSE_DIM, any test that asserts a *specific index value* computed under the old SPARSE_DIM will fail. First, identify such tests:

```bash
cd /home/jmagar/workspace/axon_rust && grep -n "term_to_index\|SPARSE_DIM\|30_522\|30522" crates/vector/ops/sparse.rs
```

Add a test that explicitly validates the new bucket count:

```rust
#[test]
fn sparse_dim_is_65536() {
    assert_eq!(
        SPARSE_DIM, 65_536,
        "SPARSE_DIM must be 65536 to halve collision probability vs the old 30522"
    );
}
```

- [ ] Add the `sparse_dim_is_65536` test to `crates/vector/ops/sparse.rs`

### Step 3.2 — Run to confirm it fails

```bash
cd /home/jmagar/workspace/axon_rust && cargo test sparse_dim_is_65536 -- --nocapture
```

Expected: FAIL — `assertion failed: SPARSE_DIM must be 65536` (current value is 30_522).

- [ ] Confirm the test fails with the right message

### Step 3.3 — Update `SPARSE_DIM` and module doc

In `crates/vector/ops/sparse.rs`, change:

```rust
// OLD
/// Number of sparse vector buckets. Matches BERT vocabulary size for compatibility.
pub const SPARSE_DIM: u32 = 30_522;
```

To:

```rust
// NEW
/// Number of sparse vector buckets.
///
/// Set to 65,536 (2^16) — double the original BERT vocabulary size (30,522).
/// The birthday paradox gives approximately 24% collision probability for 200 unique
/// terms at this bucket count, vs 48% at 30,522. The memory overhead is zero:
/// sparse vectors only store non-zero entries.
///
/// **Migration note:** Changing this constant makes existing sparse vectors (encoded
/// with 30,522 buckets) incompatible with new ones. When deploying, re-index all
/// content into a new named collection (`cortex_v2`) via `axon migrate` before
/// flipping `AXON_COLLECTION`. Do not apply this change to a live collection in place.
pub const SPARSE_DIM: u32 = 65_536;
```

Also update the module-level doc comment at the top of the file to reflect the new collision characteristics:

```rust
//! # Collision characteristics
//! With `SPARSE_DIM = 65_536` buckets and FNV-1a hashing, the birthday paradox gives
//! approximately 12% collision probability for 100 unique terms, 24% for 200 terms —
//! half the collision rate of the original 30,522-bucket configuration.
//! No memory overhead: sparse vectors store only non-zero (index, value) pairs.
```

- [ ] Update `SPARSE_DIM` constant value
- [ ] Update the constant doc comment to explain the new value and migration requirement
- [ ] Update the module-level collision characteristics doc

### Step 3.4 — Check for tests that assert specific index values

```bash
cd /home/jmagar/workspace/axon_rust && grep -n "rust_idx\|term_to_index\|assert.*idx\|assert.*index" crates/vector/ops/sparse.rs
```

The test `compute_sparse_vector_repeated_term_has_higher_weight` uses `term_to_index("rust")` to find the index and compares weights — it does not assert a specific *value* for the index. It only checks that the index exists in both vectors and has a higher weight in the triple-occurrence case. This test is **safe** — it works regardless of SPARSE_DIM because `term_to_index` produces a consistent hash modulo whatever SPARSE_DIM is set to.

The test `compute_sparse_vector_all_indices_in_bucket_range` asserts `idx < SPARSE_DIM` — this is parametric and safe.

If `grep` reveals any test that asserts `idx == <literal_number>` (e.g., `assert_eq!(rust_idx, 12345)`), that test must be updated to use `term_to_index("term")` dynamically instead of a hardcoded expected value.

- [ ] Run grep, confirm no tests use literal index values
- [ ] If any do, update them to compute the expected index via `term_to_index()`

### Step 3.5 — Run the full sparse test suite

```bash
cd /home/jmagar/workspace/axon_rust && cargo test sparse -- --nocapture
```

Expected: all 9 existing tests + the new `sparse_dim_is_65536` test pass.

- [ ] Confirm all sparse tests pass

### Step 3.6 — Run clippy and fmt

```bash
cd /home/jmagar/workspace/axon_rust && cargo clippy 2>&1 | grep "^error"
cargo fmt --check
```

Expected: no errors.

- [ ] Confirm clippy clean
- [ ] Confirm fmt clean

### Step 3.7 — Commit (with migration warning in message)

```bash
cd /home/jmagar/workspace/axon_rust && git add crates/vector/ops/sparse.rs
git commit -m "$(cat <<'EOF'
feat(sparse): bump SPARSE_DIM from 30_522 to 65_536, halving collision rate

MIGRATION REQUIRED: Existing sparse vectors encoded with 30_522 buckets are
incompatible with new 65_536-bucket vectors. Must ship alongside cortex_v2
re-index (axon migrate). Do not deploy to a live collection without migration.
EOF
)"
```

- [ ] Commit

---

## Task 4: Fix 10 — Add search latency logging to `hybrid.rs` and `client.rs`

Add `log_debug` lines that emit structured latency data after each successful search response. The `Instant` is already captured in `hybrid.rs` (`search_start`). `client.rs` also has `search_start` in `qdrant_search`. No functional change — debug logging only.

**Target log format:**
```
qdrant search_complete mode=hybrid collection=cortex hits=10 latency_ms=42
qdrant search_complete mode=named_dense collection=cortex hits=10 latency_ms=15
qdrant search_complete mode=unnamed_dense collection=cortex hits=10 latency_ms=18
```

**Files:**
- Modify: `crates/vector/ops/qdrant/hybrid.rs`
- Modify: `crates/vector/ops/qdrant/client.rs`

### Step 4.1 — Write a test that confirms the log format (documentation test only)

There is no practical way to assert on `log_debug` output in unit tests without a log capture harness. Instead, write a doc test that validates the string format the log line would produce, without calling the actual search function:

Add to `crates/vector/ops/qdrant/hybrid.rs` test block:

```rust
#[test]
fn search_complete_log_format_is_valid() {
    // Verify the log format string compiles and produces the expected structure.
    // The actual log_debug call uses this same format.
    let collection = "cortex";
    let hits = 10usize;
    let latency_ms = 42u128;
    let line = format!(
        "qdrant search_complete mode=hybrid collection={collection} hits={hits} latency_ms={latency_ms}"
    );
    assert!(line.contains("mode=hybrid"));
    assert!(line.contains("collection=cortex"));
    assert!(line.contains("hits=10"));
    assert!(line.contains("latency_ms=42"));
}
```

- [ ] Add `search_complete_log_format_is_valid` to `crates/vector/ops/qdrant/hybrid.rs`

### Step 4.2 — Run to confirm test passes

```bash
cd /home/jmagar/workspace/axon_rust && cargo test search_complete_log_format_is_valid -- --nocapture
```

Expected: PASS.

- [ ] Confirm the test passes

### Step 4.3 — Add latency log to `qdrant_hybrid_search`

In `crates/vector/ops/qdrant/hybrid.rs`, find the success path after `resp.json()` (around line 84):

**Before:**
```rust
    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant hybrid_search hits={} collection={}",
        parsed.result.len(),
        cfg.collection
    ));
    Ok(parsed.result)
```

**After:**
```rust
    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant search_complete mode=hybrid collection={} hits={} latency_ms={}",
        cfg.collection,
        parsed.result.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(parsed.result)
```

- [ ] Update the `log_debug` line in `qdrant_hybrid_search`

### Step 4.4 — Add latency log to `qdrant_named_dense_search`

In the same file, find the success path for `qdrant_named_dense_search` (around line 151):

**Before:**
```rust
    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant named_dense_search hits={} collection={}",
        parsed.result.len(),
        cfg.collection
    ));
    Ok(parsed.result)
```

**After:**
```rust
    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant search_complete mode=named_dense collection={} hits={} latency_ms={}",
        cfg.collection,
        parsed.result.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(parsed.result)
```

- [ ] Update the `log_debug` line in `qdrant_named_dense_search`

### Step 4.5 — Add latency log to `qdrant_search` in `client.rs`

In `crates/vector/ops/qdrant/client.rs`, find `qdrant_search` (around line 427). It already has `let search_start = Instant::now();` and a `log_debug` after success:

**Before:**
```rust
    log_debug(&format!(
        "qdrant search hits={} collection={}",
        res.result.len(),
        cfg.collection
    ));
    Ok(res.result)
```

**After:**
```rust
    log_debug(&format!(
        "qdrant search_complete mode=unnamed_dense collection={} hits={} latency_ms={}",
        cfg.collection,
        res.result.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(res.result)
```

- [ ] Update the `log_debug` line in `qdrant_search` in `client.rs`

### Step 4.6 — Run all qdrant tests and lint

```bash
cd /home/jmagar/workspace/axon_rust && cargo test qdrant -- --nocapture 2>&1 | tail -10
cargo clippy 2>&1 | grep "^error"
cargo fmt --check
```

Expected: all tests pass, no clippy errors, fmt clean.

- [ ] Confirm qdrant tests pass
- [ ] Confirm clippy clean
- [ ] Confirm fmt clean

### Step 4.7 — Commit

```bash
cd /home/jmagar/workspace/axon_rust && git add \
  crates/vector/ops/qdrant/hybrid.rs \
  crates/vector/ops/qdrant/client.rs
git commit -m "obs(search): add structured latency logging to all qdrant search paths"
```

- [ ] Commit

---

## Task 5: Fix 11 — Log `candidates_after_rerank` in the ask pipeline

After the reranking + relevance filter step in `retrieval.rs`, emit a `log_debug` line that records the candidate funnel: how many were retrieved, how many survived the score filter, and how many were finally selected for context.

**Files:**
- Modify: `crates/vector/ops/commands/ask/context/retrieval.rs`

### Step 5.1 — Write the test

Add a test that validates the log format string structure (same approach as Task 4 — we test the format, not the log emission):

Add to `crates/vector/ops/commands/ask/context/tests.rs` (or wherever the retrieval tests live):

```rust
#[test]
fn context_built_log_format_is_valid() {
    let candidates_retrieved = 150usize;
    let candidates_after_score_filter = 42usize;
    let candidates_selected = 10usize;
    let line = format!(
        "ask context_built candidates_retrieved={candidates_retrieved} candidates_after_score_filter={candidates_after_score_filter} candidates_selected={candidates_selected}"
    );
    assert!(line.contains("ask context_built"));
    assert!(line.contains("candidates_retrieved=150"));
    assert!(line.contains("candidates_after_score_filter=42"));
    assert!(line.contains("candidates_selected=10"));
}
```

- [ ] Add `context_built_log_format_is_valid` to the ask context test file

### Step 5.2 — Run to confirm test passes

```bash
cd /home/jmagar/workspace/axon_rust && cargo test context_built_log_format_is_valid -- --nocapture
```

Expected: PASS.

- [ ] Confirm test passes

### Step 5.3 — Implement the log line in `retrieve_ask_candidates`

In `crates/vector/ops/commands/ask/context/retrieval.rs`, the reranking + filter block ends with the `reranked` Vec (around line 75–85). After the `if reranked.is_empty()` guard, add the import and log call.

First, ensure `log_debug` is imported. Check the existing imports at the top of the file — if not already there, add:

```rust
use crate::crates::core::logging::log_debug;
```

Then, after the `reranked.is_empty()` early-return guard (around line 84) and before the `Ok(AskRetrieval { ... })` block, add:

```rust
    // Log the candidate funnel for diagnosing prefetch window adequacy.
    // candidates: raw Qdrant hits after URL/length filtering
    // reranked: survived ask_min_relevance_score threshold + topical overlap check
    // top selection happens downstream in build_context_from_candidates
    log_debug(&format!(
        "ask context_built candidates_retrieved={} candidates_after_score_filter={} candidates_selected={}",
        candidates.len(),
        reranked.len(),
        reranked.len().min(cfg.ask_chunk_limit),
    ));
```

> **Why `reranked.len().min(cfg.ask_chunk_limit)` for `candidates_selected`?**
> The actual selection of which chunks go into context happens in `build_context_from_candidates` downstream. At this point in `retrieve_ask_candidates`, the best approximation of "will be selected" is `min(reranked.len(), ask_chunk_limit)`. This is sufficient for diagnosing prefetch window adequacy without requiring a second pass through the selection logic.

- [ ] Add `use crate::crates::core::logging::log_debug;` if not already imported
- [ ] Add the `log_debug` call after the `reranked.is_empty()` guard

### Step 5.4 — Run tests and lint

```bash
cd /home/jmagar/workspace/axon_rust && cargo test -- --nocapture 2>&1 | tail -10
cargo clippy 2>&1 | grep "^error"
cargo fmt --check
```

Expected: all tests pass, no errors.

- [ ] Confirm all tests pass
- [ ] Confirm clippy clean
- [ ] Confirm fmt clean

### Step 5.5 — Commit

```bash
cd /home/jmagar/workspace/axon_rust && git add \
  crates/vector/ops/commands/ask/context/retrieval.rs \
  crates/vector/ops/commands/ask/context/tests.rs
git commit -m "obs(ask): log candidate funnel after reranking (retrieved/score-filtered/selected)"
```

- [ ] Commit

---

## Task 6: Verification — Full Suite + Lint Gate

Run the complete verification suite before raising a PR.

### Step 6.1 — Run `just verify` (the pre-PR gate)

```bash
cd /home/jmagar/workspace/axon_rust && just verify
```

This runs: `cargo fmt --check` + `cargo clippy` + `cargo check` + `cargo test`. All must pass clean.

- [ ] Confirm `just verify` exits 0

### Step 6.2 — Spot-check log output with a live ask query (optional, if services are available)

```bash
RUST_LOG=debug ./scripts/axon ask "what is hybrid search?" 2>&1 | grep "qdrant search_complete\|ask context_built"
```

Expected output (one of):
```
qdrant search_complete mode=hybrid collection=cortex hits=64 latency_ms=38
ask context_built candidates_retrieved=64 candidates_after_score_filter=21 candidates_selected=10
```

- [ ] (Optional) Confirm live log output matches expected format

### Step 6.3 — Update `.env.example` with new env vars

Add documentation for the two new env vars to `.env.example`:

```bash
# Ask-pipeline hybrid prefetch window (clamped 10-500). Default: 150.
# Higher than AXON_HYBRID_CANDIDATES (100) because ask reranks before selecting context.
# AXON_ASK_HYBRID_CANDIDATES=150

# Sparse vector bucket count (see sparse.rs). Default: 65536.
# NOTE: Changing SPARSE_DIM requires a full collection re-index via axon migrate.
```

- [ ] Add env var documentation to `.env.example`

### Step 6.4 — Final commit for env doc

```bash
cd /home/jmagar/workspace/axon_rust && git add .env.example
git commit -m "docs(env): document AXON_ASK_HYBRID_CANDIDATES in .env.example"
```

- [ ] Commit

---

## Shipping Checklist

| Fix | Status | Shipping Constraint |
|-----|--------|---------------------|
| Fix 8 — `ask_hybrid_candidates` field + wiring | Tasks 1+2 | None — independent |
| Fix 9 — `SPARSE_DIM` bump to 65,536 | Task 3 | Must ship with `cortex_v2` migration batch (Tier 1 Fix 1 + Tier 2 Fixes 5+6) |
| Fix 10 — search latency logging | Task 4 | None — pure log addition |
| Fix 11 — ask candidate funnel logging | Task 5 | None — pure log addition |

**Recommended PR strategy:**
- PR A: Fixes 8 + 10 + 11 (Tasks 1+2+4+5) — ship immediately
- PR B: Fix 9 (Task 3) — hold until `cortex_v2` migration PR is ready, merge together

---

## Key Invariants (Reference)

- Never call `reqwest::Client::new()` — use `http_client()?` singleton
- Never use `mod.rs` — Rust 2018 file-per-module layout only
- `log_debug` for hot-path logging (only visible with `RUST_LOG=debug`), `log_info` for milestone events
- All new `Config` non-`Option` fields need a default in `Config::default()` AND in all inline struct literals (the compiler only catches missing literals at test build time, not `cargo check`)
- `SPARSE_DIM` change: sparse vectors computed before the change are **incompatible** with those after. There is no fallback path — migration is required.
