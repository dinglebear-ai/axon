# Code Quality Review: `crates/jobs/`

**Date:** 2026-03-15
**Scope:** All `.rs` files in `/home/jmagar/workspace/axon_rust/crates/jobs/`
**Reviewer:** Code Review Agent (Opus 4.6)
**Files Reviewed:** 66

---

## Summary

The `crates/jobs/` module is a well-architected async job system with solid fundamentals: type-safe status enums, advisory-lock schema migrations, two-phase stale job recovery, semaphore-bounded concurrency, and centralized heartbeat management. The codebase demonstrates mature patterns for production reliability.

That said, the review identified **1 critical**, **5 high**, **9 medium**, and **8 low** severity findings across six analysis categories.

---

## Critical

### C-1: Monolith Policy Violation -- `crawl/runtime/worker/process.rs` (654 lines)

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/crawl/runtime/worker/process.rs`
**Category:** Maintainability / Policy

The file is 654 lines, exceeding the hard 500-line monolith policy limit. Functions like `process_job_impl` and `run_active_crawl_job` are large and orchestrate multiple concerns (Redis cancel polling, Chrome fallback, progress tracking, partial result saving).

**Fix:** Split into at least two files:
- `process.rs` -- top-level `process_job_impl` and `run_active_crawl_job` orchestration
- `cancel.rs` -- `poll_cancel_key`, `reconnect_cancel_redis`, `save_partial_cancel_result`
- Alternatively, move `validate_output_dir` and `spawn_progress_task` into `postprocess.rs` or a new `helpers.rs`

---

## High

### H-1: Duplicated Redis Cancel Signal Pattern

**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/embed.rs` (lines ~110-160, `cancel_embed_job`)
- `/home/jmagar/workspace/axon_rust/crates/jobs/extract.rs` (lines ~110-160, `cancel_extract_job`)
- `/home/jmagar/workspace/axon_rust/crates/jobs/crawl/runtime/db.rs` (lines ~150-200, `cancel_job`)

**Category:** Code Duplication

All three cancel functions follow an identical pattern: open Redis connection with timeout, set a cancel key with TTL, match nested `Result` chains. The code is copy-pasted with only the table name and key prefix varying.

**Fix:** Extract a shared `set_cancel_signal(cfg, table, id)` function in `common/` that encapsulates the Redis timeout, connection, and SET EX logic. Each module calls it with its `JobTable` variant.

### H-2: Duplicated Schema Initialization Strategies

**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/embed.rs` -- `OnceLock` + inline DDL
- `/home/jmagar/workspace/axon_rust/crates/jobs/refresh.rs` -- `OnceLock` + inline DDL
- `/home/jmagar/workspace/axon_rust/crates/jobs/watch.rs` -- `OnceLock` + advisory-lock DDL
- `/home/jmagar/workspace/axon_rust/crates/jobs/graph/schema.rs` -- advisory-lock DDL, no `OnceLock`
- `/home/jmagar/workspace/axon_rust/crates/jobs/ingest/schema.rs` -- advisory-lock DDL, no `OnceLock`
- `/home/jmagar/workspace/axon_rust/crates/jobs/crawl/runtime.rs` -- advisory-lock DDL, no `OnceLock`

**Category:** Inconsistency / Technical Debt

Three different schema initialization patterns coexist:
1. `OnceLock` guard + raw DDL (embed, refresh)
2. `OnceLock` guard + advisory-lock DDL (watch)
3. Advisory-lock DDL only, no `OnceLock` (graph, ingest, crawl)

The `OnceLock`-without-advisory-lock variants (embed, refresh) are vulnerable to concurrent schema init races across multiple worker processes. The advisory-lock-without-`OnceLock` variants issue unnecessary DB roundtrips on every call within the same process.

**Fix:** Standardize on `OnceLock` + advisory-lock DDL for all job types. The `OnceLock` prevents redundant DB calls within a process; the advisory lock serializes across processes. The `watch.rs` pattern is the correct one -- propagate it to all other modules.

### H-3: `process_graph_job` Contains Duplicated LLM/Non-LLM Branches

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/worker.rs` (lines 361-393)
**Category:** Code Duplication

The `if let Some(result) = llm_result` / `else` block duplicates nearly identical code for writing documents, entities, mentions, similarity, and result JSON. The only difference is whether `relationships` are written and included in the count.

**Fix:** Extract the common write sequence into a helper function, e.g. `write_graph_to_neo4j(...)`, that optionally accepts relationships. This eliminates ~30 lines of duplication.

### H-4: `mark_job_failed` Error Handling Boilerplate

**Files:** Nearly every `process_*_job` function across all workers.
**Category:** Code Duplication / Verbosity

The pattern:
```rust
if let Err(e2) = mark_job_failed(&pool, TABLE, id, &format!("...")).await {
    log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
}
```
appears approximately 25+ times across the codebase. The inner `mark_job_failed` failure is logged but otherwise identical everywhere.

**Fix:** Create a wrapper in `common/`:
```rust
pub(crate) async fn mark_failed_logged(pool: &PgPool, table: JobTable, id: Uuid, msg: &str) {
    if let Err(e) = mark_job_failed(pool, table, id, msg).await {
        log_warn(&format!("mark_job_failed failed job_id={id} error={e}"));
    }
}
```
This cuts each call site from 3 lines to 1.

### H-5: Watch Module Uses Raw Status Strings Instead of `JobStatus` Enum

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/watch.rs` (lines 12-14)
**Category:** Inconsistency / Type Safety

```rust
pub const WATCH_RUN_STATUS_RUNNING: &str = "running";
pub const WATCH_RUN_STATUS_COMPLETED: &str = "completed";
pub const WATCH_RUN_STATUS_FAILED: &str = "failed";
```

The codebase established `JobStatus` enum as the canonical way to reference status strings (documented in CLAUDE.md as a hard rule). The watch module bypasses this with raw string constants.

Additionally, `RefreshJob::job_status()` (refresh.rs:111-119) manually parses status strings back into the enum with a match statement, which is fragile and duplicates logic that should live on `JobStatus` itself (e.g., `JobStatus::from_str`).

**Fix:** Delete the `WATCH_RUN_STATUS_*` constants; use `JobStatus::Running.as_str()`, etc. Add a `FromStr` or `TryFrom<&str>` impl to `JobStatus` and use it in `RefreshJob::job_status()`.

---

## Medium

### M-1: `PgPool` Created Per-Call in Public API Functions

**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/ingest/ops.rs` -- every public function calls `make_pool(cfg)`
- `/home/jmagar/workspace/axon_rust/crates/jobs/refresh.rs` -- `get_refresh_job`, `list_refresh_jobs`, etc.
- `/home/jmagar/workspace/axon_rust/crates/jobs/refresh/schedule.rs` -- every public function calls `make_pool(cfg)`
- `/home/jmagar/workspace/axon_rust/crates/jobs/watch.rs` -- `create_watch_def`, `list_watch_defs`, etc.

**Category:** Performance / Technical Debt

While the CLAUDE.md documents the "PgPool -- Create Once, Pass Down" pattern for workers, the public-facing CRUD functions (called from CLI handlers) create a new pool on every invocation. `make_pool` includes a 5-second connect timeout, so each CLI call pays connection establishment cost.

**Fix:** For CLI-invoked functions this is acceptable (short-lived process, single call). However, for functions called in loops or from the watch worker's tick (which calls `make_pool` on every 30s tick via `run_watch_scheduler_tick`), pass the pool down. The watch worker partially does this with `run_watch_tick_with_pool` but `run_watch_scheduler_tick` still creates a fresh pool each tick.

### M-2: Inconsistent Error Return Types Across Workers

**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/graph/worker.rs` -- `run_graph_worker` returns `anyhow::Result<()>`
- `/home/jmagar/workspace/axon_rust/crates/jobs/embed/worker.rs` -- `run_embed_worker` returns `Result<(), Box<dyn Error>>`
- `/home/jmagar/workspace/axon_rust/crates/jobs/refresh/worker.rs` -- `run_refresh_worker` returns `Result<(), Box<dyn Error>>`

**Category:** Inconsistency

The graph worker uses `anyhow::Result` while all other workers use `Box<dyn Error>`. This forces unnecessary `.map_err(|err| anyhow::anyhow!("{err}"))` conversions at the graph worker boundaries.

**Fix:** Standardize on one error type. Since `worker_lane::run_job_worker` returns `Result<(), Box<dyn Error>>`, the graph worker should match. Replace `anyhow::Result<()>` with `Result<(), Box<dyn Error>>` in `run_graph_worker`.

### M-3: `cleanup_refresh_jobs` Uses Unbounded Delete Loop

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/refresh.rs` (lines 268-289)
**Category:** Performance

```rust
loop {
    let deleted = sqlx::query("DELETE FROM axon_refresh_jobs WHERE id IN (SELECT id ... LIMIT 1000)")
        ...
    total += deleted;
    if deleted == 0 { break; }
}
```

While the batched approach avoids locking the entire table, `cleanup_ingest_jobs` (ingest/ops.rs:105-120) does a single unbounded `DELETE` with no batching. The inconsistency is minor, but the refresh version has no safeguard against running indefinitely if rows keep appearing (e.g., from concurrent inserts of failed jobs).

**Fix:** Add a maximum iteration count (e.g., 100 batches = 100K rows max) as a safety valve. Alternatively, standardize both modules on the same approach.

### M-4: `refresh.rs` Schema Does Not Use Advisory Lock

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/refresh.rs` (lines 148-227)
**Category:** Reliability

The `ensure_schema` function runs multiple DDL statements (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`, `ALTER TABLE ADD COLUMN IF NOT EXISTS`) without wrapping them in an advisory-lock transaction. While `IF NOT EXISTS` / `IF NOT EXISTS` makes individual statements idempotent, concurrent execution from multiple worker processes could cause transient errors if two workers race on `ALTER TABLE ADD COLUMN`.

**Fix:** Use `begin_schema_migration_tx(pool, REFRESH_SCHEMA_LOCK_KEY)` like the watch, graph, and crawl modules do.

### M-5: Neo4j Writes in `graph/worker.rs` Are Sequential Per-Entity

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/worker.rs` (lines 178-282)
**Category:** Performance

`write_document_and_chunks`, `write_entities`, `write_chunk_mentions`, and `write_entity_relationships` all iterate sequentially, issuing one Neo4j query per item. For a document with 50 chunks and 30 entities, this means 80+ sequential Neo4j roundtrips.

**Fix:** Use Cypher `UNWIND` to batch writes. For example:
```cypher
UNWIND $entities AS e
MERGE (entity:Entity {name: e.name})
SET entity.entity_type = e.entity_type, entity.confidence = e.confidence, entity.updated_at = datetime()
```
This reduces N roundtrips to 1 per write category.

### M-6: `drain_playlist_videos_with_pool` Silently Skips Failed Videos

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/ingest/process.rs` (lines 158-160)
**Category:** Observability

Failed videos are logged with `log_warn` but not tracked in the progress JSON persisted to the database. The `completed_urls` set only includes successful videos, so there is no way to see which videos failed via `axon ingest status`.

**Fix:** Add a `failed_urls` field to the progress JSON:
```rust
"failed_urls": failed_urls.iter().collect::<Vec<_>>(),
```

### M-7: `process_refresh_job` Uses `&reqwest::Client` From `RefreshUrlContext` But Creates Client Via `http_client()`

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/refresh/url_processor.rs` (line 17, `client: &'a reqwest::Client`)
vs.
**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/refresh/processor.rs` (line 295, `http_client()`)

**Category:** Minor Inconsistency

`refresh_one_url` takes a `&reqwest::Client` parameter, but `process_refresh_job` creates a new client via `http_client()` rather than reusing the global `HTTP_CLIENT` LazyLock. The `http_client()` function may or may not return the same instance depending on implementation. This is not a bug but could lead to connection pool fragmentation.

**Fix:** Verify `http_client()` returns the LazyLock singleton. If it creates a new client each time, switch to the shared instance.

### M-8: `watch_worker.rs` Has No Backpressure or Error Retry

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/watch_worker.rs` (lines 90-105)
**Category:** Reliability

The watch worker loop sleeps a fixed 30 seconds between ticks regardless of whether the previous tick succeeded or failed. There is no backoff on repeated failures, no limit on consecutive errors, and no alerting beyond `log_warn`.

**Fix:** Add exponential backoff on consecutive errors (reset on success), similar to the AMQP reconnect pattern in `worker_lane.rs`.

### M-9: `graph/worker.rs` Creates Neo4j Client Twice

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/worker.rs`
- Line 403: `Neo4jClient::from_config(&cfg)` inside `process_claimed_graph_job` (per job)
- Line 445: `Neo4jClient::from_config(cfg)` inside `run_graph_worker` (at startup)

**Category:** Performance / Wasted Work

The startup creation (line 445) validates Neo4j is reachable and sets up the schema, but its client is never passed to the `ProcessFn` closure. Instead, `process_claimed_graph_job` creates a brand new Neo4j client for every single job.

**Fix:** The `ProcessFn` type signature `(Config, PgPool, Uuid)` does not accommodate passing additional state like a Neo4j client. Options:
1. Store the Neo4j client in `Config` (if it is `Clone + Send + Sync`)
2. Use `Arc<Neo4jClient>` captured by the closure
3. Accept the overhead if Neo4j connection pooling makes creation cheap

---

## Low

### L-1: `client: &'a reqwest::Client` Field in `RefreshUrlContext` Should Be Owned or `Arc`

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/refresh/url_processor.rs` (line 23)
**Category:** API Design

The reference lifetime ties `RefreshUrlContext` to the stack frame of `process_refresh_job`. This is fine today but constrains future refactoring (e.g., parallelizing URL processing). `reqwest::Client` is cheap to clone (internally `Arc`'d).

**Fix:** Change to `pub client: reqwest::Client` (owned, cloneable).

### L-2: `ingest/types.rs` Has a `status()` Method That Duplicates `RefreshJob::job_status()`

**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/ingest/types.rs` -- `IngestJob::status()`
- `/home/jmagar/workspace/axon_rust/crates/jobs/refresh.rs` -- `RefreshJob::job_status()`

**Category:** Code Duplication (Minor)

Both methods parse a raw `status: String` field into `JobStatus`. The logic is identical.

**Fix:** Implement `FromStr` on `JobStatus` and call it from both. Or derive `sqlx::Type` on `JobStatus` so sqlx maps it directly.

### L-3: `graph/taxonomy.rs` Uses `std::fs::read_to_string` (Blocking I/O)

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/taxonomy.rs` (line 79)
**Category:** Async Correctness

`Taxonomy::from_path` uses synchronous `fs::read_to_string` which blocks the tokio runtime thread. For small taxonomy files this is negligible, but it violates the project's "all file I/O must be `tokio::fs`" rule.

**Fix:** Change to `tokio::fs::read_to_string` and make `from_path` async, or use `tokio::task::spawn_blocking` to wrap the sync read.

### L-4: Magic Numbers in `graph/extract.rs` Type Ranking

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/extract.rs` (lines 100-114)
**Category:** Readability

The `rank()` function uses magic numbers (5, 4, 3, 2, 1, 0) without explanation of why `service` outranks `framework`.

**Fix:** Add a brief comment explaining the ranking rationale, or use named constants.

### L-5: `watch.rs` Test Cleanup Uses Raw SQL DELETE

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/watch.rs` (lines 376-379, 413-416, 449-452)
**Category:** Test Quality

Tests clean up with `DELETE FROM axon_watch_defs WHERE id=$1` but do not handle the case where cleanup fails. If test assertions fail before cleanup, rows accumulate in the shared test DB.

**Fix:** Use a test transaction that rolls back, or generate unique names (already done) and accept eventual cleanup via `cleanup_*` commands.

### L-6: `graph.rs` `enqueue_graph_job` Marks Job Failed on AMQP Failure

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph.rs` (lines 68-77)
**Category:** Design

When AMQP enqueue fails, the job is immediately marked `failed`. Other modules (ingest, refresh) log a warning and rely on polling fallback to pick up the job. The graph module is the only one that fails hard on AMQP unavailability.

**Fix:** Align with the other modules: log a warning and let polling fallback handle it. The polling lane will find the `pending` row and process it.

### L-7: `embed.rs` Dedup Check Queries for Both Pending AND Fresh Running Jobs

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/embed.rs`
**Category:** Documentation

The dedup logic that checks for existing pending/running jobs with the same input is well-implemented but lacks a doc comment explaining the rationale and the "fresh running" window.

**Fix:** Add a brief doc comment explaining why dedup is necessary and what "fresh" means (e.g., running jobs started within the last N minutes).

### L-8: Unused `_collection` Parameter in `build_recommend_request`

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/graph/similarity.rs` (line 25)
**Category:** Dead Code

```rust
pub fn build_recommend_request(
    _collection: &str,
    ...
```

The `_collection` parameter is never used in the function body.

**Fix:** Remove the parameter and update call sites, or use it in the request body if it was intended.

---

## Positive Observations

These patterns are worth calling out as exemplary:

1. **Two-phase stale watchdog** (`common/watchdog.rs`): The mark-then-confirm approach with configurable grace period prevents false-positive reclaims during legitimate long-running jobs.

2. **Centralized heartbeat via `wrap_with_heartbeat`** (`worker_lane.rs`): Individual workers never need to manage heartbeat lifecycle. This is clean separation of concerns.

3. **`FOR UPDATE SKIP LOCKED`** pattern for job claiming: Correctly avoids contention in multi-lane workers.

4. **Advisory-lock schema migrations** (`common/schema.rs`): Transaction-scoped locks that auto-release prevent DDL races without manual cleanup.

5. **Bounded channels everywhere**: The `channel(256)` policy prevents hidden OOM risks from unbounded producers.

6. **`ProcessFn` abstraction**: The generic worker lane cleanly separates job dispatch from job processing, enabling code reuse across embed, extract, refresh, ingest, and graph workers.

7. **Resume support for YouTube playlists** (`ingest/process.rs`): Progress persistence via `result_json` with `completed_urls` tracking enables restart resilience.

8. **SSRF validation in refresh processor**: `validate_url()` called before every fetch, `validate_output_dir` with path traversal prevention.

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total files reviewed | 66 |
| Total findings | 23 |
| Critical | 1 |
| High | 5 |
| Medium | 9 |
| Low | 8 |
| Lines of code (estimated) | ~8,500 |
| Monolith violations | 1 (process.rs at 654 lines) |
| Functions exceeding 80-line warning | ~3 |
| Functions exceeding 120-line hard limit | 0 |
