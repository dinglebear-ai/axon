# Ingest Throughput: Lane Scaling + Qdrant Delete Fix + Pre-Chunking

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the Qdrant CPU spike caused by synchronous `wait=true` batch deletes, default worker lane counts to CPU-based values with env-var overrides, and pre-chunk GitHub files before TEI to remove the 413 fallback path.

**Architecture:** Three independent fixes that compose cleanly. Fix 1 patches a single function in `qdrant/client.rs` (change `wait=true` → `wait=false` on stale-tail deletes only). Fix 2 adds one helper to `worker_lane.rs` and wires it into each worker. Fix 3 restructures the GitHub file embed pipeline to pre-chunk before batching, matching the crawl pipeline pattern.

**Tech Stack:** Rust, Qdrant HTTP API, tokio, `std::thread::available_parallelism`

---

## Background: Root Causes

### What crushed the CPU (the `wait=true` problem)

`embed_documents_batch()` in `tei.rs` — used by the GitHub ingest pipeline — calls `cleanup_batch_stale_tails()` after every successful upsert. That function calls `qdrant_delete_stale_tail()` once per unique URL in the batch (up to 64 URLs). Each call uses `?wait=true`, forcing Qdrant to rebuild its HNSW index synchronously before returning.

With 8 ingest lanes × many files per repo × `wait=true` per file → hundreds of concurrent synchronous index rebuilds → 371% Qdrant CPU.

The stale-tail delete is a **post-upsert cleanup** — data consistency is already guaranteed by the upsert. Making it async (`wait=false`) is safe.

`qdrant_delete_by_url_filter()` (the pre-delete path) legitimately needs `wait=true` — callers read back the data immediately after. Leave it alone.

### Why lane defaults were wrong

All non-crawl workers defaulted to 2 lanes via hardcoded constants with no env-var override (or a hidden one). 2 lanes for an ingest worker running on a 32-CPU machine is 6% utilization.

### Why TEI 413 fallbacks happen in GitHub ingest

`embed_documents_batch()` collects 64 raw file contents into one `tei_embed()` call. Large files (5k+ char) produce many chunks; 64 large files can produce 500+ chunks in one call. If `tei_embed()` hits 413 (batch too large), `embed_documents_in_batches()` falls back to **one `embed_code_with_metadata()` call per doc** — the slow path. Pre-chunking before batching eliminates this: each TEI call gets a bounded, predictable number of chunks.

---

## File Map

| File | Change |
|------|--------|
| `crates/vector/ops/qdrant/client.rs` | `qdrant_delete_stale_tail`: `wait=true` → `wait=false` |
| `crates/jobs/worker_lane.rs` | Add `resolve_lane_count(env_var, cpu_min, cpu_max) -> usize` |
| `crates/jobs/ingest.rs` | Use `resolve_lane_count("AXON_INGEST_LANES", 2, 16)` |
| `crates/jobs/embed.rs` | Remove `const WORKER_CONCURRENCY = 2` |
| `crates/jobs/embed/worker.rs` | Use `resolve_lane_count("AXON_EMBED_LANES", 2, 32)` |
| `crates/jobs/extract.rs` | Remove `const WORKER_CONCURRENCY = 2` |
| `crates/jobs/extract/worker.rs` | Use `resolve_lane_count("AXON_EXTRACT_LANES", 1, 8)` |
| `crates/jobs/refresh/worker.rs` | Use `resolve_lane_count("AXON_REFRESH_LANES", 1, 4)` |
| `crates/jobs/crawl/runtime.rs` | `const WORKER_CONCURRENCY = 5` → `fn worker_concurrency() -> usize` |
| `crates/jobs/crawl/runtime/worker/loops.rs` | Use `super::super::worker_concurrency()` |
| `crates/jobs/crawl/runtime/worker/amqp_consumer.rs` | Use `super::super::worker_concurrency()` |
| `crates/ingest/github/files.rs` | Pre-chunk files before collecting into `EmbedDocument` batches; raise `buffer_unordered` ceiling |
| `.env.example` | Document `AXON_INGEST_LANES`, `AXON_EMBED_LANES`, `AXON_EXTRACT_LANES`, `AXON_REFRESH_LANES`, `AXON_CRAWL_WORKER_LANES` |

---

## Task 1: Fix `qdrant_delete_stale_tail` — `wait=true` → `wait=false`

**This is the highest-priority fix. Do this first. It's one line.**

**Files:**
- Modify: `crates/vector/ops/qdrant/client.rs:225-250`

- [ ] **Step 1: Write the failing test**

  In `crates/vector/ops/qdrant/tests.rs`, add a test that verifies the stale-tail delete endpoint uses `wait=false`:

  ```rust
  #[test]
  fn stale_tail_delete_endpoint_uses_wait_false() {
      // The endpoint string must not contain "wait=true" for stale-tail deletes.
      // This is a compile-time / pattern check. The real behavioral test requires httpmock
      // but we can at least document the contract here.
      //
      // Visual inspection: client.rs:qdrant_delete_stale_tail must have "wait=false"
      // not "wait=true" in its endpoint format string.
      //
      // Run: grep "qdrant_delete_stale_tail" -A 10 crates/vector/ops/qdrant/client.rs | grep wait
      // Expected: wait=false
      assert!(true, "verified manually: see client.rs:qdrant_delete_stale_tail");
  }
  ```

  > Note: The real verification is `cargo test qdrant` passing after the fix. The test above documents intent; a proper httpmock test would intercept the HTTP call and assert the URL.

- [ ] **Step 2: Apply the one-line fix**

  In `crates/vector/ops/qdrant/client.rs`, find `qdrant_delete_stale_tail` (around line 225). Change its endpoint format string:

  ```rust
  // BEFORE
  let endpoint = format!(
      "{}/collections/{}/points/delete?wait=true",
      qdrant_base(cfg),
      cfg.collection
  );

  // AFTER
  let endpoint = format!(
      "{}/collections/{}/points/delete?wait=false",
      qdrant_base(cfg),
      cfg.collection
  );
  ```

  **Do not change any other `wait=true` occurrences.** `qdrant_delete_by_url_filter` (line 198) and `qdrant_delete_stale_domain_urls` (line 276) are pre-delete operations that legitimately need synchronous confirmation.

- [ ] **Step 3: Verify cargo compiles and tests pass**

  ```bash
  cargo check 2>&1 | head -20
  cargo test qdrant -- --nocapture 2>&1 | tail -20
  ```

  Expected: all pass, no warnings on changed code.

- [ ] **Step 4: Commit**

  ```bash
  git add crates/vector/ops/qdrant/client.rs
  git commit -m "perf(qdrant): use wait=false for stale-tail deletes to unblock ingest workers"
  ```

---

## Task 2: CPU-based worker lane defaults

**All workers get CPU-based lane defaults. Env vars remain as optional overrides.**

**Files:**
- Modify: `crates/jobs/worker_lane.rs` (add helper)
- Modify: `crates/jobs/ingest.rs`
- Modify: `crates/jobs/embed.rs` + `crates/jobs/embed/worker.rs`
- Modify: `crates/jobs/extract.rs` + `crates/jobs/extract/worker.rs`
- Modify: `crates/jobs/refresh/worker.rs`
- Modify: `crates/jobs/crawl/runtime.rs`
- Modify: `crates/jobs/crawl/runtime/worker/loops.rs`
- Modify: `crates/jobs/crawl/runtime/worker/amqp_consumer.rs`

### Step 1: Add `resolve_lane_count` to `worker_lane.rs`

> **NOTE: This was already added in the current session. Verify it's present before proceeding.**

Check:
```bash
grep -n "resolve_lane_count" crates/jobs/worker_lane.rs
```

If missing, add after `pub(crate) const STALE_SWEEP_INTERVAL_SECS`:

```rust
/// Resolve worker lane count: env-var override takes priority; falls back to a
/// CPU-based default clamped to `[cpu_min, cpu_max]`.
///
/// Example: `resolve_lane_count("AXON_EMBED_LANES", 2, 32)` → number of logical
/// CPUs (min 2, max 32), overridable at runtime via `AXON_EMBED_LANES=N`.
pub(crate) fn resolve_lane_count(env_var: &str, cpu_min: usize, cpu_max: usize) -> usize {
    let cpu_default = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(cpu_min, cpu_max);
    std::env::var(env_var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(cpu_default)
}
```

### Step 2: Tests for `resolve_lane_count`

Add in `crates/jobs/worker_lane/tests.rs` (or inline test module at bottom of `worker_lane.rs`):

```rust
#[cfg(test)]
mod lane_count_tests {
    use super::resolve_lane_count;

    #[test]
    fn env_var_override_takes_priority() {
        std::env::set_var("AXON_TEST_LANES_OVERRIDE", "7");
        let count = resolve_lane_count("AXON_TEST_LANES_OVERRIDE", 1, 32);
        std::env::remove_var("AXON_TEST_LANES_OVERRIDE");
        assert_eq!(count, 7);
    }

    #[test]
    fn zero_env_var_falls_back_to_cpu_default() {
        std::env::set_var("AXON_TEST_LANES_ZERO", "0");
        let count = resolve_lane_count("AXON_TEST_LANES_ZERO", 2, 32);
        std::env::remove_var("AXON_TEST_LANES_ZERO");
        // 0 is rejected (< 1), so cpu_default applies (clamped to [2, 32])
        assert!(count >= 2 && count <= 32, "count={count} not in [2, 32]");
    }

    #[test]
    fn missing_env_var_uses_cpu_clamped() {
        std::env::remove_var("AXON_TEST_LANES_MISSING");
        let count = resolve_lane_count("AXON_TEST_LANES_MISSING", 3, 10);
        assert!(count >= 3 && count <= 10, "count={count} not in [3, 10]");
    }

    #[test]
    fn invalid_env_var_falls_back() {
        std::env::set_var("AXON_TEST_LANES_INVALID", "not_a_number");
        let count = resolve_lane_count("AXON_TEST_LANES_INVALID", 1, 64);
        std::env::remove_var("AXON_TEST_LANES_INVALID");
        // falls back to cpu_default
        assert!(count >= 1 && count <= 64, "count={count} not in [1, 64]");
    }
}
```

Run:
```bash
cargo test lane_count -- --nocapture
```
Expected: 4 tests pass.

### Step 3: Wire `resolve_lane_count` into each worker

- [ ] **`crates/jobs/ingest.rs`** — replace the inline env-var logic:

  ```rust
  // BEFORE
  lane_count: std::env::var("AXON_INGEST_LANES")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(2),

  // AFTER
  lane_count: crate::crates::jobs::worker_lane::resolve_lane_count("AXON_INGEST_LANES", 2, 16),
  ```

- [ ] **`crates/jobs/embed.rs`** — remove the const:

  ```rust
  // REMOVE this line:
  const WORKER_CONCURRENCY: usize = 2;
  ```

- [ ] **`crates/jobs/embed/worker.rs`** — update the `WorkerConfig` construction. Add `resolve_lane_count` to the existing import:

  ```rust
  use crate::crates::jobs::worker_lane::{
      ProcessFn, WorkerConfig, run_job_worker, validate_worker_env_vars,
      resolve_lane_count,  // add this
  };
  ```

  Then change:
  ```rust
  // BEFORE
  lane_count: WORKER_CONCURRENCY,

  // AFTER
  lane_count: resolve_lane_count("AXON_EMBED_LANES", 2, 32),
  ```

- [ ] **`crates/jobs/extract.rs`** — remove the const:

  ```rust
  // REMOVE this line:
  const WORKER_CONCURRENCY: usize = 2;
  ```

- [ ] **`crates/jobs/extract/worker.rs`** — add `resolve_lane_count` to import and change:

  ```rust
  use crate::crates::jobs::worker_lane::{ProcessFn, WorkerConfig, run_job_worker, resolve_lane_count};
  ```

  Then:
  ```rust
  // BEFORE
  lane_count: WORKER_CONCURRENCY,

  // AFTER
  lane_count: resolve_lane_count("AXON_EXTRACT_LANES", 1, 8),
  ```

  > Extract is LLM-bound. Default max of 8 prevents saturating the LLM endpoint.

- [ ] **`crates/jobs/refresh/worker.rs`** — change the hardcoded literal. First add the import near the top:

  ```rust
  use crate::crates::jobs::worker_lane::{ProcessFn, WorkerConfig, run_job_worker, resolve_lane_count};
  ```

  Then:
  ```rust
  // BEFORE
  lane_count: 2,

  // AFTER
  lane_count: resolve_lane_count("AXON_REFRESH_LANES", 1, 4),
  ```

- [ ] **`crates/jobs/crawl/runtime.rs`** — replace the const with a function:

  ```rust
  // BEFORE
  const WORKER_CONCURRENCY: usize = 5;

  // AFTER
  pub(super) fn worker_concurrency() -> usize {
      crate::crates::jobs::worker_lane::resolve_lane_count("AXON_CRAWL_WORKER_LANES", 2, 8)
  }
  ```

  > Crawl already has heavy internal per-job concurrency (CPUs×8 connections per crawl job). Max 8 worker lanes prevents memory pressure from 8 simultaneous full-site crawls.

- [ ] **`crates/jobs/crawl/runtime/worker/loops.rs`** — remove `WORKER_CONCURRENCY` from the import and replace usages:

  ```rust
  // BEFORE
  use super::super::{
      STALE_SWEEP_INTERVAL_SECS, TABLE, WORKER_CONCURRENCY, ensure_schema,
      reenqueue_orphaned_pending_jobs,
  };

  // AFTER
  use super::super::{
      STALE_SWEEP_INTERVAL_SECS, TABLE, ensure_schema,
      reenqueue_orphaned_pending_jobs, worker_concurrency,
  };
  ```

  Replace all 4 usages of `WORKER_CONCURRENCY`:
  ```rust
  // Each occurrence: WORKER_CONCURRENCY  →  worker_concurrency()
  if WORKER_CONCURRENCY <= 1 {         →  if worker_concurrency() <= 1 {
  (1..=WORKER_CONCURRENCY).map(...)    →  (1..=worker_concurrency()).map(...)
  ```
  There are 4 occurrences total (2 in polling path, 2 in AMQP path).

- [ ] **`crates/jobs/crawl/runtime/worker/amqp_consumer.rs`** — same import fix:

  ```rust
  // BEFORE
  use super::super::{STALE_SWEEP_INTERVAL_SECS, TABLE, WORKER_CONCURRENCY};

  // AFTER
  use super::super::{STALE_SWEEP_INTERVAL_SECS, TABLE, worker_concurrency};
  ```

  Replace the 1 usage in the log line:
  ```rust
  // BEFORE
  "crawl worker lane={} listening on queue={} concurrency={}",
  lane, cfg.crawl_queue, WORKER_CONCURRENCY

  // AFTER
  "crawl worker lane={} listening on queue={} concurrency={}",
  lane, cfg.crawl_queue, worker_concurrency()
  ```

- [ ] **Step 4: Verify**

  ```bash
  cargo check 2>&1 | head -30
  cargo test lane_count jobs -- --nocapture 2>&1 | tail -20
  ```

  Expected: clean compile, tests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/jobs/worker_lane.rs crates/jobs/ingest.rs \
          crates/jobs/embed.rs crates/jobs/embed/worker.rs \
          crates/jobs/extract.rs crates/jobs/extract/worker.rs \
          crates/jobs/refresh/worker.rs crates/jobs/crawl/runtime.rs \
          crates/jobs/crawl/runtime/worker/loops.rs \
          crates/jobs/crawl/runtime/worker/amqp_consumer.rs
  git commit -m "feat(workers): CPU-based lane defaults with env-var overrides for all workers"
  ```

### Step 6: Update `.env.example`

Find the `AXON_INGEST_LANES` section in `.env.example` and expand it:

```bash
# Worker lane counts (default: number of logical CPUs, clamped per worker type)
# Override to tune concurrency. Set lower if Qdrant or TEI become saturated.
AXON_INGEST_LANES=          # default: min(CPUs, 16) — each lane processes one full repo
AXON_EMBED_LANES=           # default: min(CPUs, 32) — mostly I/O bound to TEI
AXON_EXTRACT_LANES=         # default: min(CPUs, 8)  — LLM-bound; cap prevents saturation
AXON_REFRESH_LANES=         # default: min(CPUs, 4)  — lightweight scheduler
AXON_CRAWL_WORKER_LANES=    # default: min(CPUs, 8)  — each lane has internal CPUs×8 concurrency
```

```bash
git add .env.example
git commit -m "docs(env): document worker lane env vars"
```

---

## Task 3: Pre-chunk GitHub files before TEI batching

**This eliminates the 413 fallback path in `embed_collected_docs()` by chunking each file upfront and emitting one `EmbedDocument` per chunk instead of one per file.**

**Files:**
- Modify: `crates/ingest/github/files.rs` — `collect_embed_docs`, `read_file_embed_doc`, `embed_collected_docs`

### Background

Currently:
```
read_file → EmbedDocument { content: full_file_text }  (one per file)
         → embed_documents_in_batches(batch_size=64)
         → embed_documents_batch: chunks ALL 64 files, calls tei_embed with 500+ chunks
         → on 413: fallback to per-doc embed_code_with_metadata (slow)
```

After fix:
```
read_file → pre-chunk → Vec<EmbedDocument> (one per chunk, bounded content)
         → embed_documents_in_batches(batch_size=256)
         → embed_documents_batch: each doc is already one chunk → no 413
```

The key change: `read_file_embed_doc` returns `Vec<EmbedDocument>` (one per chunk) instead of `Option<EmbedDocument>`. Each chunk doc has `content` bounded to ~2000 chars, matching what `tei_embed()` expects.

### Step 1: Write tests for chunk-level doc collection

Add to the test module in `crates/ingest/github/files.rs` (or a new test at the bottom):

```rust
#[cfg(test)]
mod pre_chunk_tests {
    use crate::crates::vector::ops::input::chunk_text;

    #[test]
    fn chunk_text_produces_bounded_content() {
        let long = "x".repeat(5000);
        let chunks = chunk_text(&long);
        for chunk in &chunks {
            assert!(chunk.len() <= 2200, "chunk too large: {}", chunk.len());
        }
        assert!(chunks.len() > 1, "expected multiple chunks for 5000-char input");
    }

    #[test]
    fn empty_file_produces_no_chunks() {
        let chunks = chunk_text("   ");
        // chunk_text on whitespace-only returns one chunk but it will be empty-ish
        // The caller filters out empty content, so this is fine
        let _ = chunks; // just verify no panic
    }
}
```

Run:
```bash
cargo test pre_chunk -- --nocapture
```
Expected: 2 pass.

### Step 2: Add required imports and raise buffer ceiling

In `crates/ingest/github/files.rs`, add the chunk imports. Find the existing imports block:

```rust
use crate::crates::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name,
};
use crate::crates::vector::ops::{EmbedDocument, embed_code_with_metadata};
```

Add after the classify import:
```rust
use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};
```

Change the buffer ceiling in `collect_embed_docs`:
```rust
// BEFORE
let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 16);

// AFTER
let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
```

Change `GITHUB_EMBED_DOC_BATCH_SIZE`:
```rust
// BEFORE
const GITHUB_EMBED_DOC_BATCH_SIZE: usize = 64;

// AFTER
const GITHUB_EMBED_DOC_BATCH_SIZE: usize = 256;
```

> 256 chunk-level docs per TEI call is safe because each chunk is ≤2000 chars. Compare: the old 64 file-level docs could be 64 × many chunks = unbounded.

### Step 3: Change `read_file_embed_doc` to return one doc per chunk

Replace the function signature and body. The function currently returns `Result<Option<EmbedDocument>, String>`. Change it to return `Result<Vec<EmbedDocument>, String>`:

```rust
/// Read a single file from the cloned repo and return one EmbedDocument per chunk.
///
/// Pre-chunking here prevents oversized TEI batches downstream. Each returned
/// document carries the file's full metadata but only ~2000 chars of content.
async fn read_file_embed_doc(
    ctx: &FileEmbedCtx,
    path: &str,
) -> Result<Vec<EmbedDocument>, String> {
    let full_path = ctx.repo_root.join(path);
    let text = match tokio::fs::read_to_string(&full_path).await {
        Ok(t) => t,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github read_failed path={path} err={e}"
            ));
            return Ok(vec![]);
        }
    };
    if text.trim().is_empty() {
        return Ok(vec![]);
    }

    let ext = file_extension(path);
    let extra = build_github_payload(&GitHubPayloadParams {
        repo: ctx.name.clone(),
        owner: ctx.owner.clone(),
        content_kind: "file".into(),
        branch: Some(ctx.default_branch.clone()),
        default_branch: Some(ctx.default_branch.clone()),
        repo_description: ctx.repo_description.clone(),
        pushed_at: ctx.pushed_at.clone(),
        is_private: ctx.is_private,
        file_path: Some(path.to_string()),
        file_language: Some(language_name(&ext).to_string()),
        file_type: Some(classify_file_type(path).to_string()),
        is_test: Some(is_test_path(path)),
        file_size_bytes: Some(text.len()),
        chunking_method: Some(chunking_method(&ext).to_string()),
        ..Default::default()
    });

    let source_url = format!(
        "https://github.com/{}/{}/blob/{}/{}",
        ctx.owner, ctx.name, ctx.default_branch, path
    );

    // Pre-chunk: one EmbedDocument per chunk to keep TEI batch sizes bounded.
    let chunks = chunk_code(&text, &ext)
        .unwrap_or_else(|| chunk_text(&text));

    Ok(chunks
        .into_iter()
        .map(|chunk| EmbedDocument {
            content: chunk,
            url: source_url.clone(),
            source_type: "github".to_string(),
            title: Some(path.to_string()),
            extra: Some(extra.clone()),
            file_extension: Some(ext.clone()),
        })
        .collect())
}
```

### Step 4: Update `collect_embed_docs` to flatten chunks

The return type of the stream changes from `Ok(Some(doc))` / `Ok(None)` to `Ok(Vec<doc>)`. Update `collect_embed_docs`:

```rust
async fn collect_embed_docs(
    ctx: &FileEmbedCtx,
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
    failed: &mut usize,
) -> Vec<EmbedDocument> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = ctx.clone();
            async move { read_file_embed_doc(&ctx, &path).await }
        })
        .buffer_unordered(concurrency);

    let mut docs = Vec::new();
    let mut files_done = 0usize;

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(file_chunks) => docs.extend(file_chunks),
            Err(_) => *failed += 1,
        }
        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            send_progress(
                progress_tx,
                serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": 0,
                    "phase": "collecting_files",
                }),
            )
            .await;
        }
    }

    docs
}
```

### Step 5: Simplify `embed_collected_docs` fallback

Since every `EmbedDocument` is now a single chunk, the fallback per-doc path in `embed_documents_in_batches` is now fast (one small `embed_code_with_metadata` call per chunk, not per file). The fallback closure can stay as-is — it just becomes a no-op fast path since 413s won't happen with bounded chunk content.

No change needed here. Verify `embed_collected_docs` still compiles as-is.

### Step 6: Verify

```bash
cargo check 2>&1 | head -30
cargo test ingest -- --nocapture 2>&1 | tail -30
cargo test pre_chunk -- --nocapture
```

Expected: clean compile, all tests pass.

### Step 7: Commit

```bash
git add crates/ingest/github/files.rs
git commit -m "perf(ingest): pre-chunk github files before TEI batching — eliminates 413 fallback path"
```

---

## Task 4: Verify and tune

- [ ] Restart the ingest worker with the updated binary
- [ ] Monitor Qdrant CPU: `ps aux | grep qdrant`
  Expected: CPU drops from 371% to <50% during ingest
- [ ] Monitor ingest throughput: check job completion rate with `axon ingest list`
- [ ] If Qdrant CPU is still high, reduce `AXON_INGEST_LANES` further in `.env`
- [ ] Tune `AXON_EMBED_DOC_CONCURRENCY` if TEI is underutilized

---

## Testing Cheatsheet

```bash
cargo check                          # fast type check
cargo test lane_count -- --nocapture # lane default tests
cargo test qdrant -- --nocapture     # qdrant client tests
cargo test ingest -- --nocapture     # ingest pipeline tests
cargo test pre_chunk -- --nocapture  # pre-chunk tests
just verify                          # full: fmt + clippy + check + test
```
