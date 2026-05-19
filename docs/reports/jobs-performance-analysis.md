# Performance and Scalability Analysis: `crates/jobs/`

**Date**: 2026-03-15
**Scope**: `crates/jobs/` (~16.8k lines, 66 files) -- AMQP job workers backed by RabbitMQ (lapin) + PostgreSQL (sqlx)
**Analyst**: Performance engineering review

---

## Table of Contents

1. [Critical Findings](#critical-findings)
2. [High Severity Findings](#high-severity-findings)
3. [Medium Severity Findings](#medium-severity-findings)
4. [Low Severity Findings](#low-severity-findings)
5. [Architecture Strengths](#architecture-strengths)

---

## Critical Findings

### C-1: PgPool Created Per CLI CRUD Operation (~60 call sites)

**Files**: `embed.rs`, `extract.rs`, `ingest/ops.rs`, `refresh.rs`, `refresh/schedule.rs`, `watch.rs`, `crawl/runtime/db.rs`, `common/stats.rs`

**Problem**: Every public CRUD function (`get_embed_job`, `list_embed_jobs`, `cancel_embed_job`, `cleanup_embed_jobs`, `get_extract_job`, `list_extract_jobs`, `start_ingest_job`, `get_ingest_job`, `list_ingest_jobs`, `cancel_ingest_job`, `cleanup_ingest_jobs`, etc.) calls `make_pool(cfg).await?` -- creating a **new PgPool with TCP handshake** on every invocation. There are **60+ call sites** across the crate doing this.

`make_pool` creates a pool with `min_connections(2)`, meaning each call establishes at minimum 2 TCP connections to Postgres, performs TLS negotiation (if configured), and runs the connection protocol. Under any kind of list/status polling, this creates massive connection churn.

**Impact**: Under load (e.g., MCP server or web UI polling job status every 2s), this creates ~30 new TCP connections per minute per table. With 6 job tables, that is ~180 connections/min to Postgres just for status checks. Connection creation takes 5-50ms depending on TLS and network, so each CRUD call adds 10-100ms of pure overhead before the actual query runs.

**Estimated latency impact**: +10-100ms per CRUD call (dominates the <1ms query time for a UUID lookup).

**Recommendation**: Introduce a shared `LazyLock<PgPool>` or pass a pool from the caller. The worker paths already do this correctly (they call `make_pool` once at startup). Only the CLI/MCP-facing CRUD functions are affected.

```rust
// Option A: Module-level lazy pool (simplest migration)
use std::sync::OnceLock;
use tokio::sync::OnceCell;

static SHARED_POOL: OnceCell<PgPool> = OnceCell::const_new();

async fn shared_pool(cfg: &Config) -> Result<&PgPool, anyhow::Error> {
    SHARED_POOL.get_or_try_init(|| make_pool(cfg)).await
}

// Option B: Add pool parameter to all public CRUD functions (more explicit)
pub async fn get_embed_job(pool: &PgPool, id: Uuid) -> Result<Option<EmbedJob>, Box<dyn Error>> {
    // ...use pool directly...
}
```

The `_with_pool` variants already exist for most functions (e.g., `start_embed_job_with_pool`). The public wrappers just need to share a pool instead of creating one each time.

---

### C-2: Neo4j Client Created Per Graph Job

**File**: `graph/worker.rs:402-419` (`process_claimed_graph_job`)

**Problem**: `Neo4jClient::from_config(&cfg)` is called inside `process_claimed_graph_job`, which runs for **every single graph job**. This means a new Neo4j bolt connection (TCP handshake + auth) is opened per job, used, and dropped.

The graph worker's `run_graph_worker` function at line 445 already creates a `Neo4jClient` at startup for schema validation, but that client is never passed into the process function. Instead, `process_claimed_graph_job` creates its own.

**Impact**: Neo4j bolt connection setup is 20-100ms (TCP + auth). For a burst of 100 graph jobs, that is 2-10s of pure connection overhead. The connection is also not pooled, so under concurrent lane execution, all lanes compete for new connections.

**Estimated latency impact**: +20-100ms per graph job.

**Recommendation**: Create the Neo4j client once in `run_graph_worker` and pass it through the `ProcessFn`. Since `ProcessFn` takes `Config` by value, the simplest approach is to store the client in a `static OnceCell` or use `Arc<Neo4jClient>`:

```rust
// In run_graph_worker:
let neo4j = Arc::new(Neo4jClient::from_config(cfg)?
    .ok_or_else(|| anyhow::anyhow!("Neo4j required"))?);

let neo4j_clone = neo4j.clone();
let process_fn: ProcessFn = Arc::new(move |cfg, pool, id| {
    let neo4j = neo4j_clone.clone();
    Box::pin(async move {
        process_graph_job_with_client(&cfg, &neo4j, &pool, id).await;
    })
});
```

---

### C-3: Sequential Neo4j Writes in Graph Worker (N+1 Pattern)

**Files**: `graph/worker.rs:178-282` (`write_document_and_chunks`, `write_entities`, `write_chunk_mentions`, `write_entity_relationships`), `graph/similarity.rs:115-132`

**Problem**: Every Neo4j write function iterates over its input and issues **one Cypher query per item** sequentially. For a document with 50 chunks, 30 entities, and 40 relationships:

- `write_document_and_chunks`: 50 sequential MERGE queries
- `write_entities`: 30 sequential MERGE queries
- `write_chunk_mentions`: 50 chunks x ~5 entity mentions each = ~250 sequential MERGE queries
- `write_entity_relationships`: 40 sequential MERGE queries
- `write_similarity_edges`: variable, ~20 sequential MERGE queries

Total: ~390 sequential round-trips to Neo4j per graph job.

**Impact**: At 2-5ms per Cypher query round-trip, a single graph job takes 780ms-1.95s purely in Neo4j I/O. With UNWIND batching, this could be reduced to 5 queries total (~10-25ms).

**Estimated latency impact**: 10-40x improvement possible per graph job.

**Recommendation**: Use Neo4j `UNWIND` for batch operations:

```cypher
// Before (N queries):
// for chunk in chunks { MERGE (d:Document {url: $url}) MERGE (c:Chunk...) ... }

// After (1 query):
UNWIND $chunks AS chunk
MERGE (d:Document {url: $url})
SET d.source_type = $source_type, d.collection = $collection, d.updated_at = datetime()
MERGE (c:Chunk {point_id: chunk.point_id})
SET c.url = $url, c.collection = $collection, c.chunk_index = chunk.chunk_index, c.updated_at = datetime()
MERGE (c)-[:BELONGS_TO]->(d)
```

Apply the same pattern to `write_entities`, `write_chunk_mentions`, `write_entity_relationships`, and `compute_similarity`.

---

## High Severity Findings

### H-1: Config Cloned Per Job Dispatch (50+ String Fields)

**Files**: `worker_lane/delivery.rs:50`, `worker_lane/poll.rs:83`, `crawl/runtime/worker/loops.rs:206`

**Problem**: `ProcessFn` is defined as:
```rust
type ProcessFn = Arc<dyn Fn(Config, PgPool, Uuid) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync>;
```

`Config` is taken **by value**, so every job dispatch clones the entire Config struct. The Config struct has ~139 public fields including 24 `String` fields (heap-allocated) plus `Vec<String>` fields like `positional`, `custom_headers`, `exclude_path_prefix`, and `PathBuf` fields. Each clone involves:
- 24 String heap allocations + memcpy
- Multiple Vec heap allocations
- PathBuf heap allocation

At the call sites:
- `delivery.rs:50`: `process_fn(cfg.clone(), pool.clone(), job_id)` -- every AMQP delivery
- `poll.rs:83`: `process_fn(cfg.clone(), pool.clone(), id)` -- every polling claim

**Impact**: ~2-5 microseconds per clone (24 heap allocations). Under sustained throughput of 100 jobs/sec, this is ~0.5ms/sec of pure allocation overhead. Not catastrophic alone, but it causes unnecessary allocator pressure and GC-like fragmentation.

**Estimated impact**: Low per-job, but compounding under burst load. ~2-5us per job.

**Recommendation**: Change `ProcessFn` to take `&Config` or `Arc<Config>`:

```rust
type ProcessFn = Arc<dyn Fn(Arc<Config>, PgPool, Uuid) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync>;
```

The `Config` is already owned by the worker and never mutated during job processing. `Arc<Config>` makes the clone a reference count increment (~1 atomic operation) instead of 24+ heap allocations.

---

### H-2: New AMQP Connection Per `enqueue_job` / `batch_enqueue_jobs` Call

**File**: `common/amqp.rs:89-132`

**Problem**: `batch_enqueue_jobs` (and by extension `enqueue_job`) opens a **new AMQP TCP connection** for every enqueue operation:

```rust
let (conn, ch) = open_amqp_connection_and_channel(cfg, queue_name).await?;
// ... publish ...
conn.close(200, "".into()).await;
```

This is documented in the code comments as intentional for "short-lived operations," but enqueuing happens frequently:
- After every crawl/embed/extract job creation
- During sitemap backfill (potentially hundreds of URLs)
- During orphaned job re-enqueue at worker startup
- During watchdog reclaim re-enqueue

**Impact**: AMQP TCP connection setup is 5-20ms. For a crawl that discovers 500 sitemap URLs and enqueues 500 embed jobs, that is 500 connections x 10ms = 5 seconds of pure connection overhead (even with `batch_enqueue_jobs` batching per call, the callers often call it per-job via `enqueue_job`).

**Estimated latency impact**: +5-20ms per enqueue call.

**Recommendation**: For the worker path (which already has a long-lived AMQP connection), publish directly on the existing channel. For the CLI path, consider a module-level cached connection with TTL:

```rust
// Workers: reuse existing channel
pub async fn enqueue_job_on_channel(ch: &Channel, queue_name: &str, job_id: Uuid) -> Result<()> {
    ch.basic_publish(/* ... */).await?;
    Ok(())
}
```

---

### H-3: Refresh Schema Migration Not Transactional

**File**: `refresh.rs:148-227` (`ensure_schema`)

**Problem**: Unlike `embed.rs`, `extract.rs`, and `watch.rs` which use `begin_schema_migration_tx` with advisory locks, the refresh module's `ensure_schema` runs DDL statements **without a transaction or advisory lock**:

```rust
async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE TABLE IF NOT EXISTS axon_refresh_jobs ...").execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS ...").execute(pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS axon_refresh_targets ...").execute(pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS axon_refresh_schedules ...").execute(pool).await?;
    // ... more DDL ...
}
```

If two refresh workers start simultaneously, they can race on `ALTER TABLE ADD COLUMN`, potentially causing one to fail or encountering partial schema states.

**Impact**: Race condition during worker startup. Low probability in steady state, but can cause worker startup failures during deployments.

**Recommendation**: Wrap in `begin_schema_migration_tx` like the other modules:

```rust
async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, REFRESH_SCHEMA_LOCK_KEY).await?;
    // ... all DDL inside tx ...
    tx.commit().await?;
    Ok(())
}
```

---

### H-4: Unbounded Cleanup Delete Loop (No Progress Logging, No Batch Sleep)

**Files**: `refresh.rs:268-289` (`cleanup_refresh_jobs`), `extract.rs:246-271` (`cleanup_extract_jobs`)

**Problem**: Both functions use an unbounded delete loop:

```rust
loop {
    let deleted = sqlx::query(
        "DELETE FROM ... WHERE id IN (SELECT id FROM ... WHERE status = ANY($1) LIMIT 1000)"
    ).execute(&pool).await?.rows_affected();
    total += deleted;
    if deleted == 0 { break; }
}
```

While the 1000-row batch limit prevents individual DELETE from being too large, there is:
1. No sleep between batches -- this can saturate the PG connection pool and starve other operations
2. No progress logging -- for tables with millions of failed/canceled jobs, this runs silently for minutes
3. No timeout -- if the table is huge, this blocks indefinitely

**Impact**: During cleanup of a large table (e.g., 100k failed jobs), this issues 100 DELETE queries as fast as possible, holding a connection from the pool and generating heavy WAL writes. Other queries may experience increased latency.

**Recommendation**: Add inter-batch sleep and progress logging:

```rust
loop {
    let deleted = sqlx::query(/* ... */).execute(&pool).await?.rows_affected();
    total += deleted;
    if deleted == 0 { break; }
    log_info(&format!("cleanup: deleted {total} rows so far"));
    tokio::time::sleep(Duration::from_millis(50)).await; // yield to other operations
}
```

---

## Medium Severity Findings

### M-1: Watch Worker Has No Error Backoff

**File**: `watch_worker.rs:90-105`

**Problem**: The watch worker's main loop has a fixed 30s sleep regardless of success or failure:

```rust
loop {
    match run_watch_tick_with_pool(cfg, &pool).await {
        Ok(processed) => { /* log */ }
        Err(err) => log_warn(&format!("watch scheduler tick failed: {err}")),
    }
    tokio::time::sleep(Duration::from_secs(WATCH_WORKER_DEFAULT_TICK_SECS)).await;
}
```

If the database is down, this retries every 30 seconds with no backoff. While 30s is reasonable, a persistent DB failure (e.g., Postgres restart taking 5 minutes) generates ~10 error log entries. Not critical but could be tighter.

**Impact**: Log noise during infrastructure outages. The fixed 30s interval prevents tight-loop hammering, but there is no exponential backoff on consecutive errors.

**Recommendation**: Add simple consecutive-error tracking:

```rust
let mut consecutive_errors = 0u32;
loop {
    match run_watch_tick_with_pool(cfg, &pool).await {
        Ok(_) => { consecutive_errors = 0; }
        Err(err) => {
            consecutive_errors += 1;
            log_warn(&format!("watch scheduler tick failed ({consecutive_errors}): {err}"));
        }
    }
    let delay = if consecutive_errors > 3 {
        Duration::from_secs(60) // Back off on repeated failures
    } else {
        Duration::from_secs(WATCH_WORKER_DEFAULT_TICK_SECS)
    };
    tokio::time::sleep(delay).await;
}
```

---

### M-2: `http_client()` Called Per Refresh Job and Per Graph Job Instead of Reusing

**Files**: `refresh/processor.rs:295`, `graph/extract.rs:127`, `graph/similarity.rs:75`

**Problem**: `http_client()` (from `crate::crates::core::http`) is called per-job. If this function creates a new `reqwest::Client` each time (rather than returning a `LazyLock` singleton), it creates a new connection pool and TLS context per call. Even if `http_client()` returns a cached client, the call pattern suggests the caller expects per-job construction.

Looking at the code, `graph/extract.rs:127` calls `http_client()?` per LLM extraction call, and `graph/similarity.rs:75` calls it per similarity computation. Within a single graph job, these are called once each, but across many jobs they accumulate.

**Impact**: If `http_client()` creates a new client: ~5-10ms per call for TLS context setup. If cached: negligible.

**Recommendation**: Verify `http_client()` returns a shared instance. If it does, this is a non-issue. If not, convert to `LazyLock<reqwest::Client>` or pass the client through the job context.

---

### M-3: Redis Connection Created Per Cancel Check (embed cancel path)

**File**: `embed.rs:196-253` (`cancel_embed_job`)

**Problem**: When canceling an embed job, a **new Redis connection** is created:

```rust
match redis::Client::open(cfg.redis_url.clone()) {
    Ok(redis_client) => {
        match tokio::time::timeout(Duration::from_secs(3), redis_client.get_multiplexed_async_connection()).await {
            // ...
        }
    }
}
```

The same pattern exists in `extract.rs:205-243`. Each cancel operation opens a fresh Redis TCP connection, performs one SET, and drops it.

**Impact**: Redis connection setup is ~1-5ms. For bulk cancel operations (e.g., canceling 50 jobs), this is 50-250ms of pure connection overhead.

**Recommendation**: For bulk operations, open one Redis connection and reuse it. For single cancel operations, the overhead is acceptable but could be improved with a module-level cached connection.

---

### M-4: `count_stale_and_pending_jobs` Union Scans All 4 Job Tables

**File**: `common/stats.rs:19-51`

**Problem**: The query uses `UNION ALL` across all four job tables without any index hint for the status filter:

```sql
WITH all_jobs AS (
    SELECT status, updated_at FROM axon_crawl_jobs
    UNION ALL
    SELECT status, updated_at FROM axon_extract_jobs
    UNION ALL
    SELECT status, updated_at FROM axon_embed_jobs
    UNION ALL
    SELECT status, updated_at FROM axon_ingest_jobs
)
SELECT
    COUNT(*) FILTER (WHERE status = 'running' AND updated_at < NOW() - ...) AS stale,
    COUNT(*) FILTER (WHERE status = 'pending') AS pending
FROM all_jobs
```

Only the `pending` status has a partial index (`idx_*_pending`). There is no partial index on `status = 'running'` in any table, so the `stale` count requires a sequential scan of the running rows.

**Impact**: With 10k+ jobs across tables (mostly completed), the UNION materializes all rows before filtering. Not catastrophic at current scale, but will degrade as tables grow.

**Recommendation**: Add partial indexes on `status = 'running'` for each table:

```sql
CREATE INDEX IF NOT EXISTS idx_axon_crawl_jobs_running
ON axon_crawl_jobs(updated_at ASC) WHERE status = 'running';
```

Alternatively, rewrite as 4 parallel targeted queries (one per table) and sum in Rust, which allows each query to use its table-specific indexes.

---

### M-5: Watchdog Sweep Issues Individual UPDATE Per Marked Candidate

**File**: `common/watchdog.rs:272-285`

**Problem**: The watchdog `reclaim_stale_running_jobs` function issues individual UPDATE queries for each stale candidate in the "mark" phase:

```rust
for (id, marked) in mark_batch {
    let mark_query = format!("UPDATE {} SET result_json=$2 WHERE id=$1 AND status='running'", table.as_str());
    let _ = sqlx::query(&mark_query).bind(id).bind(marked).execute(pool).await?;
    stats.marked_candidates += 1;
}
```

If there are 20 stale candidates, this issues 20 individual UPDATEs. The retry batch (line 227-246) has the same issue.

**Impact**: 20 round-trips at 1-2ms each = 20-40ms per sweep for the mark phase. Sweeps run every 30 seconds, so the absolute overhead is small, but under high stale-job scenarios (e.g., after a broker outage), there could be 50+ candidates.

**Recommendation**: The mark batch cannot easily use `ANY($1)` because each row gets a unique `result_json` payload (containing `first_seen_stale_at` timestamps). However, the retry batch at line 227 could batch using `unnest`:

```sql
UPDATE {table} SET status='pending', updated_at=NOW(), ...
WHERE id = ANY($1) AND status='running'
```

For the mark batch, the current per-row approach may be necessary due to unique payloads, but consider using a single `INSERT INTO ... SELECT` pattern with JSONB construction.

---

### M-6: Embed Dedupe Query May Not Use Optimal Index

**File**: `embed.rs:114-133`

**Problem**: The dedupe check in `start_embed_job_with_pool` queries:

```sql
SELECT id FROM axon_embed_jobs
WHERE (status = $3 OR (status = $4 AND updated_at >= NOW() - ...))
  AND input_text = $1
  AND config_json = $2
ORDER BY created_at DESC LIMIT 1
```

There is no composite index covering `(input_text, config_json, status)`. The only index is the partial `WHERE status = 'pending'` index on `created_at`. This query falls back to a sequential scan filtered on `input_text` equality, which for large text payloads involves comparing potentially multi-KB strings.

**Impact**: As the embed jobs table grows (10k+ rows), this dedupe query becomes increasingly expensive. With large `input_text` values, each row comparison involves string equality on the full text.

**Recommendation**: Add a hash-based dedupe column:

```sql
ALTER TABLE axon_embed_jobs ADD COLUMN input_hash TEXT GENERATED ALWAYS AS (md5(input_text)) STORED;
CREATE INDEX idx_axon_embed_jobs_dedupe ON axon_embed_jobs(input_hash, status);
```

---

## Low Severity Findings

### L-1: `OnceLock` Schema Guard Does Not Protect Across Processes

**Files**: `embed.rs:17`, `refresh.rs:52`, `watch.rs:9`

**Problem**: `static SCHEMA_INIT: OnceLock<()>` prevents `ensure_schema` from running more than once **within the same process**, but when multiple worker processes start (e.g., separate Docker containers or s6 services), each process independently runs schema migration. The advisory lock in `begin_schema_migration_tx` handles the cross-process case correctly, so this is just a minor efficiency note -- the in-process guard prevents redundant schema checks within a single worker's lifetime.

**Impact**: Negligible. The pattern is correct for its purpose.

---

### L-2: `format!` Used for SQL Query Construction

**Files**: Multiple (`job_ops.rs`, `watchdog.rs`, `stats.rs`, `refresh/processor.rs`)

**Problem**: Several queries use `format!` to interpolate table names and status strings:

```rust
let query = format!(
    "UPDATE {table_name} SET status='{failed}' ... WHERE id=$1 AND status='{running}'",
    failed = JobStatus::Failed.as_str(),
    running = JobStatus::Running.as_str(),
);
```

While this is safe (table names come from `JobTable::as_str()` which returns `&'static str`, and status values come from the `JobStatus` enum), it prevents query plan caching by the Postgres prepared statement cache.

**Impact**: Each `format!`-constructed query string is a unique prepared statement from Postgres's perspective. With 6 job tables x 5 operation types = ~30 unique query plans. This is well within normal Postgres limits but prevents leveraging `sqlx`'s compile-time query checking.

**Recommendation**: Accept the current approach. The dynamic table name makes compile-time `sqlx::query!` impractical. The alternative (separate functions per table) would add significant code duplication for negligible performance gain.

---

### L-3: Crawl Worker Polling Mode Is Single-Threaded Per Lane

**File**: `crawl/runtime/worker/loops.rs:47-91`

**Problem**: The crawl polling lane processes one job at a time (claim -> process -> loop). Unlike the AMQP path which uses `FuturesUnordered` for concurrent processing, the polling path is purely sequential per lane.

**Impact**: If crawl polling is active (AMQP unavailable), throughput is limited to `N` concurrent jobs where `N` = lane count. Each job must fully complete before the next is claimed. This is intentional (spider futures are `!Send`), but worth noting for scalability.

**Recommendation**: This is by design. The `!Send` constraint on spider futures prevents the same `FuturesUnordered` pattern used by embed/extract. Document the throughput implication.

---

### L-4: Ingest Progress Serializes `completed_urls` as Full Array on Every Update

**File**: `ingest/process.rs:164-170`

**Problem**: After each video completes in a YouTube playlist ingest, the progress update serializes the entire `completed_urls` HashSet:

```rust
let progress = serde_json::json!({
    "completed_urls": completed_urls.iter().collect::<Vec<_>>(),
    // ...
});
update_ingest_progress(pool, job_id, &progress).await;
```

For a 500-video playlist, the 400th update serializes a 400-element URL array and writes it to the `result_json` JSONB column. This is a growing write amplification pattern.

**Impact**: With average URL length of 80 bytes, the 400th update writes ~32KB of JSONB. Over 500 videos, total write volume is ~500 * 250 * 80 = ~10MB of JSONB writes. Manageable but unnecessary.

**Recommendation**: Store only the count and last-completed URL in the progress update. Use `completed_urls` only for resume-on-restart (written less frequently, e.g., every 25 completions).

---

## Architecture Strengths

The following patterns are well-designed and should be preserved:

1. **`FOR UPDATE SKIP LOCKED`** in `claim_next_pending` -- This is the gold standard for concurrent job claiming. No worker blocks on another's lock, and no job is double-processed.

2. **Two-pass watchdog stale detection** -- The mark-then-confirm pattern in `reclaim_stale_running_jobs` prevents false positives when heartbeats arrive between sweeps. The `observed_updated_at` comparison is a clever guard.

3. **Semaphore-based backpressure** in `worker_lane` -- The `Semaphore::new(lane_count)` pattern limits in-flight jobs without unbounded growth, and the `FuturesUnordered` drain handles completion notification efficiently.

4. **Heartbeat centralization via `wrap_with_heartbeat`** -- The worker_lane module wraps every `ProcessFn` with automatic heartbeat management, eliminating the need for individual workers to manage heartbeat lifetimes.

5. **Advisory lock schema migrations** -- Using `pg_advisory_xact_lock` for DDL serialization is the correct Postgres pattern. Transaction-scoped locks auto-release on commit/rollback.

6. **Polling fallback** -- The AMQP-primary with SQL-polling fallback is a resilient pattern. Workers continue functioning when the broker is down.

7. **Publisher confirms** -- `batch_enqueue_jobs` uses `confirm_select` + `wait_for_confirms` to ensure the broker has accepted all messages before closing the connection.

8. **Bounded channels everywhere** -- The codebase consistently uses `channel(256)` instead of `unbounded_channel`, preventing hidden backpressure bugs.

---

## Summary by Severity

| Severity | Count | Estimated Combined Impact |
|----------|-------|--------------------------|
| Critical | 3 | 10-100ms per CRUD call (C-1), 20-100ms per graph job (C-2), 10-40x graph job slowdown (C-3) |
| High | 4 | 2-5us per job dispatch (H-1), 5-20ms per enqueue (H-2), startup race (H-3), cleanup starvation (H-4) |
| Medium | 6 | Various -- log noise, missing indexes, suboptimal dedupe |
| Low | 4 | Negligible individual impact |

**Highest ROI fixes** (effort vs impact):
1. **C-3** (Neo4j UNWIND batching) -- Single largest latency reduction, moderate implementation effort
2. **C-1** (Shared PgPool for CRUD) -- Eliminates connection churn, straightforward migration
3. **C-2** (Shared Neo4j client) -- Simple fix, meaningful per-job improvement
4. **H-1** (Arc<Config>) -- Small type change with pervasive benefit
