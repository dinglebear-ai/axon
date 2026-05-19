# Embed Pipeline Resilience Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the embed pipeline resilient to per-doc TEI failures, eliminate data loss from delete-before-embed, and tune the timeout budget so TEI retries fit inside the doc timeout.

**Architecture:** Three surgical changes to `pipeline.rs` and `tei_client.rs`. Fix 1 changes the pipeline loop from fail-fast to skip-and-continue. Fix 2 replaces delete-before-embed with upsert-first-then-delete-stale-tail (matching `embed_chunks_impl`). Fix 3 lowers TEI retry defaults so the worst-case retry budget fits inside the doc timeout. `EmbedSummary` gains a `docs_failed` field to report partial results.

**Tech Stack:** Rust, tokio, Qdrant REST API, TEI HTTP embeddings

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/vector/ops/tei.rs` | Modify (lines 25-29) | Add `docs_failed` to `EmbedSummary` |
| `crates/vector/ops/tei/pipeline.rs` | Modify (primary) | Fix 1: skip-and-continue loop. Fix 2: upsert-first pattern |
| `crates/vector/ops/tei/tei_client.rs` | Modify (line 10) | Fix 3: lower `TEI_MAX_RETRIES_DEFAULT` from 10 to 5 |
| `crates/vector/ops/tei/tests.rs` | Modify | Add pipeline resilience tests |

No new files created. All changes are edits to existing files.

---

## Chunk 1: All Three Fixes + Tests

### Task 1: Add `docs_failed` to `EmbedSummary`

**Files:**
- Modify: `crates/vector/ops/tei.rs:25-29`

- [ ] **Step 1: Add `docs_failed` field to `EmbedSummary`**

In `crates/vector/ops/tei.rs`, change the `EmbedSummary` struct from:

```rust
#[derive(Debug, Clone, Copy)]
pub struct EmbedSummary {
    pub docs_embedded: usize,
    pub chunks_embedded: usize,
}
```

to:

```rust
#[derive(Debug, Clone, Copy)]
pub struct EmbedSummary {
    pub docs_embedded: usize,
    pub docs_failed: usize,
    pub chunks_embedded: usize,
}
```

- [ ] **Step 2: Fix all `EmbedSummary` construction sites**

Search for `EmbedSummary {` across the codebase and add `docs_failed: 0` to each. There are two sites:

1. `crates/vector/ops/tei/pipeline.rs:224` — this will be set to the real count in Task 3.
2. `crates/vector/ops/tei/prepare.rs` — search for `EmbedSummary` there and add `docs_failed: 0`.

Run: `cargo check 2>&1 | head -30`
Expected: clean (no errors about missing field)

- [ ] **Step 3: Commit**

```bash
git add crates/vector/ops/tei.rs crates/vector/ops/tei/pipeline.rs crates/vector/ops/tei/prepare.rs
git commit -m "refactor(embed): add docs_failed field to EmbedSummary"
```

---

### Task 2: Fix 2 — Switch to upsert-first pattern (eliminate data loss)

**Files:**
- Modify: `crates/vector/ops/tei/pipeline.rs:35-101` (`embed_prepared_doc` function)

This is the highest-priority data-safety fix. The current `embed_prepared_doc` deletes all existing Qdrant points for a URL BEFORE calling TEI. If TEI times out or errors, the old data is gone and the new data never arrived — permanent data loss for that URL until re-embed.

The safer pattern (already used in `embed_chunks_impl` in `tei.rs:113-121`) is: upsert first (deterministic UUID v5 point IDs overwrite existing), then delete stale tail chunks after.

- [ ] **Step 1: Rewrite `embed_prepared_doc` to remove pre-delete**

Replace the entire `embed_prepared_doc` function in `crates/vector/ops/tei/pipeline.rs` (lines 35-101) with:

```rust
async fn embed_prepared_doc(
    cfg: &Config,
    doc: PreparedDoc,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    let vectors = tei_embed(cfg, &doc.chunks)
        .await
        .map_err(|e| -> SendError { e.to_string().into() })?;
    if vectors.is_empty() {
        return Err(format!("TEI returned no vectors for {}", doc.url).into());
    }
    if vectors.len() != doc.chunks.len() {
        return Err(format!(
            "TEI vector count mismatch for {}: {} vectors for {} chunks",
            doc.url,
            vectors.len(),
            doc.chunks.len()
        )
        .into());
    }
    log_debug(&format!(
        "embed_doc url={} chunk_count={}",
        doc.url,
        doc.chunks.len()
    ));
    let dim = vectors[0].len();
    let chunk_count = doc.chunks.len();
    let url = doc.url.clone();
    let timestamp = Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors.into_iter()).enumerate() {
        let point_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{}:{}", url, idx).as_bytes(),
        );
        let mut payload = serde_json::json!({
            "url": url,
            "domain": doc.domain,
            "source_type": doc.source_type,
            "content_type": doc.content_type,
            "chunk_index": idx,
            "chunk_text": chunk,
            "scraped_at": timestamp,
        });
        if let Some(t) = &doc.title {
            payload["title"] = serde_json::Value::String(t.clone());
        }
        if let Some(serde_json::Value::Object(map)) = &doc.extra {
            for (k, v) in map {
                payload[k] = v.clone();
            }
        }
        points.push(serde_json::json!({
            "id": point_id.to_string(),
            "vector": vecv,
            "payload": payload,
        }));
    }
    // Return the URL and chunk count along with points so the caller can
    // run stale-tail cleanup AFTER the upsert succeeds.
    Ok((dim, url, chunk_count, points))
}
```

Key changes:
- **Removed** the `qdrant_delete_by_url_filter` call entirely
- **Removed** the `strict_predelete()` check
- Return type now includes `url: String` and `chunk_count: usize` so the caller can run `qdrant_delete_stale_tail` after upsert

- [ ] **Step 2: Update `DocFuture` type alias**

The return type changed, so update the type alias on line 32-33:

```rust
type DocFuture<'a> = Pin<
    Box<dyn Future<Output = Result<(usize, String, usize, Vec<serde_json::Value>), SendError>> + Send + 'a>,
>;
```

- [ ] **Step 3: Remove dead imports and functions**

Remove `qdrant_delete_by_url_filter` from the import on line 4 (it's no longer used in this file).
Remove the `env_bool` function (lines 14-23) and `strict_predelete` function (lines 25-28) — they are now dead code.

After cleanup, the imports should be:

```rust
use super::{EmbedProgress, EmbedSummary, PreparedDoc, qdrant_store, tei_client::tei_embed};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_delete_stale_tail};
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;
```

Note: `qdrant_delete_stale_tail` replaces `qdrant_delete_by_url_filter` in the import. `OnceLock` is removed (no longer needed without `strict_predelete`).

- [ ] **Step 4: Update timeout wrapper return type**

`embed_prepared_doc_with_timeout` must match the new return type. Change:

```rust
async fn embed_prepared_doc_with_timeout(
    cfg: &Config,
    doc: PreparedDoc,
    timeout_secs: u64,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    let url = doc.url.clone();
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        embed_prepared_doc(cfg, doc),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log_warn(&format!(
                "embed timed out after {timeout_secs}s for {url}"
            ));
            Err(format!("embed timed out after {timeout_secs}s while processing {url}").into())
        }
    }
}
```

Note: the timeout warning message no longer mentions "pre-delete may have run" because we no longer pre-delete.

- [ ] **Step 5: Verify compilation**

Run: `cargo check 2>&1 | head -30`
Expected: errors about the `run_embed_pipeline` loop (it still destructures the old 2-tuple) — that's Task 3.

- [ ] **Step 6: Commit**

```bash
git add crates/vector/ops/tei/pipeline.rs
git commit -m "fix(embed): switch to upsert-first pattern, eliminate delete-before-embed data loss"
```

---

### Task 3: Fix 1 — Make pipeline resilient to per-doc failures

**Files:**
- Modify: `crates/vector/ops/tei/pipeline.rs:126-228` (`run_embed_pipeline` function)

Currently, `result?` on line 172 means one doc timing out aborts the entire pipeline. All other docs in the batch — including ones that already completed successfully and ones that haven't started yet — are lost.

- [ ] **Step 1: Rewrite the pipeline loop to skip-and-continue**

Replace the `run_embed_pipeline` function body. The key change is replacing `result?` with a `match` that logs failures and continues:

```rust
pub(super) async fn run_embed_pipeline(
    cfg: &Config,
    prepared: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, SendError> {
    let docs_total = prepared.len();
    log_info(&format!("embed_pipeline docs={}", docs_total));
    let doc_timeout_secs = env_usize_clamped("AXON_EMBED_DOC_TIMEOUT_SECS", 300, 10, 7200) as u64;
    let doc_concurrency = env_usize_clamped(
        "AXON_EMBED_DOC_CONCURRENCY",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 16),
        1,
        64,
    );
    let flush_point_threshold = env_usize_clamped("AXON_QDRANT_POINT_BUFFER", 256, 128, 16384);

    let mut work = prepared.into_iter();
    let mut inflight: FuturesUnordered<DocFuture<'_>> = FuturesUnordered::new();
    if let Some(tx) = &progress_tx {
        let _ = tx
            .send(EmbedProgress {
                docs_total,
                docs_completed: 0,
                chunks_embedded: 0,
            })
            .await;
    }
    for _ in 0..doc_concurrency {
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
            )));
        }
    }

    let mut chunks_embedded = 0usize;
    let mut docs_completed = 0usize;
    let mut docs_failed = 0usize;
    let mut pending_points: Vec<serde_json::Value> = Vec::new();
    // Track URLs and their chunk counts for stale-tail cleanup after upsert.
    let mut stale_tail_queue: Vec<(String, usize)> = Vec::new();
    let mut collection_dim: Option<usize> = None;

    while let Some(result) = inflight.next().await {
        match result {
            Ok((dim, url, chunk_count, mut points)) => {
                match collection_dim {
                    None => {
                        if qdrant_store::collection_needs_init(&cfg.collection) {
                            qdrant_store::ensure_collection(cfg, dim)
                                .await
                                .map_err(|e| -> SendError { e.to_string().into() })?;
                        }
                        collection_dim = Some(dim);
                    }
                    Some(existing) if existing != dim => {
                        return Err(format!(
                            "TEI embedding dimension mismatch: expected {}, got {}",
                            existing, dim
                        )
                        .into());
                    }
                    _ => {}
                }
                chunks_embedded += points.len();
                pending_points.append(&mut points);
                stale_tail_queue.push((url, chunk_count));

                if pending_points.len() >= flush_point_threshold {
                    qdrant_store::qdrant_upsert(cfg, &pending_points)
                        .await
                        .map_err(|e| -> SendError { e.to_string().into() })?;
                    pending_points.clear();
                    // Stale-tail cleanup for URLs whose points just got upserted.
                    for (url, count) in stale_tail_queue.drain(..) {
                        if let Err(e) = qdrant_delete_stale_tail(cfg, &url, count).await {
                            log_warn(&format!(
                                "embed stale-tail cleanup failed for {url}: {e}"
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                docs_failed += 1;
                log_warn(&format!("embed_pipeline doc_failed: {e}"));
            }
        }

        docs_completed += 1;
        if let Some(tx) = &progress_tx {
            tx.send(EmbedProgress {
                docs_total,
                docs_completed,
                chunks_embedded,
            })
            .await
            .ok();
        }

        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
            )));
        }
    }

    // Flush remaining points.
    if !pending_points.is_empty() {
        qdrant_store::qdrant_upsert(cfg, &pending_points)
            .await
            .map_err(|e| -> SendError { e.to_string().into() })?;
        for (url, count) in stale_tail_queue.drain(..) {
            if let Err(e) = qdrant_delete_stale_tail(cfg, &url, count).await {
                log_warn(&format!(
                    "embed stale-tail cleanup failed for {url}: {e}"
                ));
            }
        }
    }

    if docs_failed > 0 {
        log_warn(&format!(
            "embed_pipeline completed with {docs_failed}/{docs_total} doc failures"
        ));
    }

    Ok(EmbedSummary {
        docs_embedded: docs_total - docs_failed,
        docs_failed,
        chunks_embedded,
    })
}
```

Key changes from the original:
1. `result?` → `match result { Ok(...) => ..., Err(e) => { docs_failed += 1; log; } }`
2. `docs_completed` now increments for both success and failure (moved after the match)
3. `stale_tail_queue` accumulates `(url, chunk_count)` pairs; cleanup runs after each upsert flush
4. `docs_embedded` in the return is `docs_total - docs_failed` (only successfully embedded docs)
5. Final summary logs the failure count if any

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | head -30`
Expected: clean (no errors)

- [ ] **Step 3: Commit**

```bash
git add crates/vector/ops/tei/pipeline.rs
git commit -m "fix(embed): skip failed docs instead of aborting entire pipeline"
```

---

### Task 4: Fix 3 — Lower TEI retry default to fit inside doc timeout

**Files:**
- Modify: `crates/vector/ops/tei/tei_client.rs:10`

The current `TEI_MAX_RETRIES_DEFAULT = 10` with exponential backoff (1s, 2s, 4s, 8s, 16s, 32s, 60s, 60s, 60s, 60s) can take ~17 minutes worst-case. The doc timeout (`AXON_EMBED_DOC_TIMEOUT_SECS`, default 300s) wraps the entire embed call including retries. With 10 retries, TEI gets through ~4-5 attempts before the doc timeout fires, wasting the remaining retry budget.

With 5 retries: worst-case is ~1s + ~2s + ~4s + ~8s + ~16s = ~31s of backoff + 5×30s request timeouts = ~181s total. This fits comfortably inside the 300s doc timeout with room for the embed/upsert work.

- [ ] **Step 1: Change the default from 10 to 5**

In `crates/vector/ops/tei/tei_client.rs`, line 10:

```rust
const TEI_MAX_RETRIES_DEFAULT: usize = 5;
```

Users who want the old behavior can still set `TEI_MAX_RETRIES=10` in their environment.

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | head -30`
Expected: clean

- [ ] **Step 3: Commit**

```bash
git add crates/vector/ops/tei/tei_client.rs
git commit -m "fix(embed): lower TEI retry default from 10 to 5 to fit inside doc timeout"
```

---

### Task 5: Remove dead code

**Files:**
- Modify: `crates/vector/ops/tei/pipeline.rs` (confirm no references to removed items)

- [ ] **Step 1: Verify `qdrant_delete_by_url_filter` is still used elsewhere**

Run: `grep -rn "qdrant_delete_by_url_filter" crates/`

If it's still used in other files (e.g. `qdrant.rs` re-export, or other callers), keep the re-export. Only remove the import from `pipeline.rs`. If it's ONLY used in `pipeline.rs`, also remove it from the re-export in `crates/vector/ops/qdrant.rs`.

- [ ] **Step 2: Verify `env_bool` and `strict_predelete` have no other callers**

Run: `grep -rn "env_bool\|strict_predelete\|AXON_EMBED_STRICT_PREDELETE" crates/`

These should only appear in `pipeline.rs`. If they're used elsewhere, keep them. If not, confirm they were removed in Task 2 Step 3.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -10`
Expected: all tests pass (the `tei_embed_*` tests in `tests.rs` test the TEI client, not the pipeline — they should be unaffected)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy 2>&1 | tail -10`
Expected: no warnings related to the changed files

- [ ] **Step 5: Commit if any cleanup was needed**

```bash
git add -A
git commit -m "chore(embed): remove dead pre-delete code and unused imports"
```

---

### Task 6: Add tests for pipeline resilience

**Files:**
- Modify: `crates/vector/ops/tei/tests.rs`

- [ ] **Step 1: Write a test verifying `EmbedSummary` includes `docs_failed`**

Add to `crates/vector/ops/tei/tests.rs`:

```rust
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
```

- [ ] **Step 2: Run it**

Run: `cargo test embed_summary_exposes_docs_failed -- --nocapture`
Expected: PASS

- [ ] **Step 3: Write a test verifying TEI retry default is 5**

```rust
/// TEI retry default must be 5 (not 10) to fit inside the doc timeout budget.
#[test]
fn tei_max_retries_default_fits_doc_timeout() {
    // The constant is private, but we can verify the env_usize_clamped call
    // returns 5 when no env override is set, by checking the worst-case
    // retry budget math:
    // 5 retries × 30s request timeout + backoff (1+2+4+8+16=31s) ≈ 181s
    // This must be < 300s (AXON_EMBED_DOC_TIMEOUT_SECS default).
    let max_retries = 5usize;
    let request_timeout_s = 30u64;
    let backoff_sum_s: u64 = (0..max_retries as u32).map(|i| 1u64.saturating_mul(2u64.pow(i)).min(60)).sum();
    let worst_case = (max_retries as u64 * request_timeout_s) + backoff_sum_s;
    assert!(
        worst_case < 300,
        "worst-case retry budget ({worst_case}s) must fit inside 300s doc timeout"
    );
}
```

- [ ] **Step 4: Run it**

Run: `cargo test tei_max_retries_default_fits_doc_timeout -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/vector/ops/tei/tests.rs
git commit -m "test(embed): add pipeline resilience and timeout budget tests"
```

---

### Task 7: Update CLAUDE.md documentation

**Files:**
- Modify: `crates/vector/CLAUDE.md`

- [ ] **Step 1: Update TEI 429 / Rate Limiting section**

Change the retry count from 10 to 5 in `crates/vector/CLAUDE.md`:

> On 429 or 503, `tei_embed()` retries up to **5 times** with exponential backoff starting at 1s (1, 2, 4, 8, 16s) + jitter. Override with `TEI_MAX_RETRIES` env var.

- [ ] **Step 2: Add a note about pipeline resilience**

Add a new subsection under "Critical Patterns":

```markdown
### Pipeline Resilience
`run_embed_pipeline()` in `tei/pipeline.rs` processes docs concurrently with per-doc timeouts. Individual doc failures (TEI timeout, transport error) are **logged and skipped** — they do not abort the remaining batch. `EmbedSummary.docs_failed` reports how many docs failed. The pipeline uses **upsert-first** (deterministic UUID v5 point IDs overwrite existing) then **stale-tail cleanup** after successful upsert — no data is deleted until the replacement is safely stored.
```

- [ ] **Step 3: Commit**

```bash
git add crates/vector/CLAUDE.md
git commit -m "docs(vector): update retry count and add pipeline resilience section"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run `just verify` (or equivalent)**

Run: `cargo fmt --check && cargo clippy && cargo check && cargo test --lib 2>&1 | tail -10`
Expected: all clean, all tests pass

- [ ] **Step 2: Verify the pipeline.rs file is under monolith limits**

Run: `wc -l crates/vector/ops/tei/pipeline.rs`
Expected: well under 500 lines

- [ ] **Step 3: Review the git log**

Run: `git log --oneline -10`
Expected: 4-6 clean commits covering the three fixes + tests + docs
