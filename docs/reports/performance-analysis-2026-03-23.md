# Axon Rust — Performance & Scalability Analysis
**Date:** 2026-03-23
**Scope:** `crates/vector/`, `crates/jobs/`, `crates/crawl/`, `crates/ingest/`, `crates/services/`, `crates/core/config/`

---

## Table of Contents

1. [TEI Embedding Hot-Path](#1-tei-embedding-hot-path)
2. [Graph Subsystem](#2-graph-subsystem)
3. [Embed Worker Pipeline](#3-embed-worker-pipeline)
4. [Config Clone Bomb](#4-config-clone-bomb)
5. [Qdrant Retry Logic — Missing Jitter](#5-qdrant-retry-logic--missing-jitter)
6. [Database Performance](#6-database-performance)
7. [Memory Management](#7-memory-management)
8. [Concurrency & Async Efficiency](#8-concurrency--async-efficiency)
9. [Crawl Worker Pipeline](#9-crawl-worker-pipeline)
10. [Scalability Barriers](#10-scalability-barriers)
11. [Summary Priority Table](#11-summary-priority-table)

---

## 1. TEI Embedding Hot-Path

### 1.1 `env_usize_clamped` Called on Every Batch — No Caching

**Severity:** High
**Estimated impact:** Eliminates 3× syscall overhead per embed batch; measurable at hundreds of batches per large job
**File:** `crates/vector/ops/tei/tei_client.rs`, lines 200–203

**Finding:**

```rust
// tei_client.rs:190 — tei_embed() is the inner loop of every embed job
pub(crate) async fn tei_embed(cfg: &Config, inputs: &[String]) -> ... {
    let batch_size    = env_usize_clamped("TEI_MAX_CLIENT_BATCH_SIZE", 128, 1, 128); // line 200
    let embed_url     = format!("{}/embed", cfg.tei_url.trim_end_matches('/'));       // line 201
    let max_attempts  = env_usize_clamped("TEI_MAX_RETRIES", TEI_MAX_RETRIES_DEFAULT, 1, 20); // line 202
    let request_timeout_ms = request_timeout_ms_from_env();                           // line 203
```

`env_usize_clamped` calls `env::var()` on every invocation. `env::var()` acquires a global process-level mutex on Linux (libc `getenv` is not thread-safe; Rust wraps it with a lock). For an embed job over a 500-document corpus this function fires hundreds of times. `request_timeout_ms_from_env()` also calls `env::var()`.

By contrast, the HNSW search parameters use the correct pattern — `LazyLock` initialized once at startup:

```rust
// utils.rs — correct pattern, already used for search
pub(crate) static HNSW_EF_SEARCH: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("AXON_HNSW_EF_SEARCH", 128, 32, 512));
```

**Recommendation:** Apply the same `LazyLock` pattern to the three embed vars:

```rust
static TEI_BATCH_SIZE: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("TEI_MAX_CLIENT_BATCH_SIZE", 128, 1, 128));
static TEI_MAX_ATTEMPTS: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("TEI_MAX_RETRIES", TEI_MAX_RETRIES_DEFAULT, 1, 20));
static TEI_REQUEST_TIMEOUT_MS: LazyLock<u64> =
    LazyLock::new(|| request_timeout_ms_from_env());
```

The `embed_url` string is rebuilt each call by formatting `cfg.tei_url`. Since `tei_url` is fixed in `Config`, this is a pure allocation waste on every batch. Cache it as a `String` in the function or pre-compute it once at the caller level.

---

### 1.2 TEI Semaphore is Process-Global but Concurrency Limit is Low

**Severity:** Medium
**Estimated impact:** 10–30% throughput increase under multi-lane embed workloads
**File:** `crates/vector/ops/tei/tei_client.rs`, line 41–42

**Finding:**

```rust
static TEI_CONCURRENCY: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(env_usize_clamped("AXON_TEI_MAX_CONCURRENT", 8, 1, 64)));
```

Default is 8 concurrent in-flight TEI requests across the entire process. With `AXON_EMBED_LANES=2` workers each running at default `AXON_EMBED_DOC_CONCURRENCY` (~8 docs), up to 16 embed futures are competing for 8 semaphore permits. This means half the pipeline is always blocked waiting for permits.

The CLAUDE.md documentation notes TEI is on steamy-wsl (RTX 4070) with a `--max-batch-tokens 163,840` budget. TEI's GPU can handle significantly more than 8 concurrent requests since they're batched on the server side.

**Recommendation:** Raise the default to 16–24 and document the tuning guidance more prominently. The clamp ceiling of 64 is appropriate. Expose via `AXON_TEI_MAX_CONCURRENT` (already done) but update the default.

---

## 2. Graph Subsystem

### 2.1 One Qdrant HTTP Call Per Document (N+1 Pattern)

**Severity:** Critical
**Estimated impact:** For a graph build over 1,000 URLs: reduces from 1,000 Qdrant round-trips to 1
**File:** `crates/jobs/graph/worker.rs`, line 319

**Finding:**

`process_graph_job` is the hot path for every graph job. It calls `qdrant_retrieve_by_url` once per URL:

```rust
// worker.rs:319 — called once per graph job, one Qdrant scroll per URL
let points = qdrant_retrieve_by_url(cfg, &url, None).await?;
```

With `graph_concurrency=4` lanes and a typical knowledge graph build processing thousands of URLs, this produces 4 concurrent Qdrant scroll requests at a time. Each scroll request traverses the collection using a filter on the `url` keyword field.

The problem is the graph worker processes jobs one URL at a time. If the user runs `axon graph build` over a domain with 500 pages, that enqueues 500 individual graph jobs, each firing a separate Qdrant scroll. The total latency is `500 * (network RTT + Qdrant scroll time)`.

**Recommendation:**

1. **Index the `url` field as a keyword index in Qdrant** if not already done. Without an explicit keyword payload index, each filter-by-URL scan is O(n) across the collection.

2. **Batch the initial Qdrant retrieval**: Consider a `build_graph_batch` operation that accepts multiple URLs and issues one scroll per batch (with a `should` filter across URLs), then partitions results by URL client-side. This converts N sequential round-trips into N/batch_size parallel ones.

3. **Pre-filter graph jobs by chunk existence**: Before enqueueing a graph job for a URL, check whether the URL has any points in Qdrant using the `/facet` endpoint (O(1)). Jobs for URLs with no vectors are no-ops and waste a round-trip.

---

### 2.2 `group_by_url_max_score` Clones Strings Unnecessarily

**Severity:** Medium
**Estimated impact:** Eliminates O(n) string clones in the similarity result aggregation loop
**File:** `crates/jobs/graph/similarity.rs`, lines 49–69

**Finding:**

```rust
pub fn group_by_url_max_score(results: Vec<(String, f32, String)>) -> Vec<SimilarityEdge> {
    let mut grouped: HashMap<String, SimilarityEdge> = HashMap::new();

    for (target_url, score, target_source_type) in results {
        let entry = grouped
            .entry(target_url.clone())  // ← clones the key even on insert
            .or_insert_with(|| SimilarityEdge {
                source_url: String::new(),
                target_url,             // moves target_url into SimilarityEdge
                score,
                target_source_type: target_source_type.clone(),  // ← clone on every insert
            });
```

The `target_url.clone()` on line 54 is redundant: `HashMap::entry()` only borrows the key; the owned value can be moved into the `SimilarityEdge` on the first insert and the clone is only needed when the key already exists (to avoid moving). The `target_source_type.clone()` on line 59 clones unconditionally on every insert regardless of whether the entry is new or existing.

**Recommendation:** Use `entry().or_insert_with_key()` or restructure to move on first insert and update the score/type in the existing entry's fields only when a higher score is found. The `or_insert_with` closure already captures the owned `target_url` — the `.clone()` for the map key is the avoidable allocation.

---

### 2.3 `compute_similarity` Builds Endpoint URL String Per Call

**Severity:** Low
**Estimated impact:** Eliminates one heap allocation per graph job; noise compared to network I/O
**File:** `crates/jobs/graph/similarity.rs`, lines 78–82

**Finding:**

```rust
pub async fn compute_similarity(cfg: &Config, neo4j: &Neo4jClient, url: &str) -> ... {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );
```

This allocates and formats the endpoint URL on every call. With `graph_concurrency=4` and thousands of graph jobs, this runs thousands of times. The `qdrant_base(cfg)` call is a string trim — also cheap but needlessly repeated.

**Recommendation:** Accept the endpoint as a `&str` parameter, or compute it once in the calling code (`process_graph_job`) and pass it down. Minor but consistent with the rest of the codebase's LazyLock patterns.

---

### 2.4 Sequential Neo4j Writes in `process_graph_job`

**Severity:** High
**Estimated impact:** 30–50% reduction in per-job latency for graph-dense documents
**File:** `crates/jobs/graph/worker.rs`, lines 385–391

**Finding:**

```rust
// worker.rs — these four Neo4j writes are strictly sequential
write_entity_relationships(neo4j, &relationships).await?;
write_document_and_chunks(neo4j, cfg, &url, &source_type, &chunks).await?;
write_entities(neo4j, &entities).await?;
let mention_count =
    write_chunk_mentions(neo4j, taxonomy, &source_type, &chunks, &entities).await?;
```

Each call is an independent Neo4j `UNWIND` + `MERGE` batch. They could be parallelized since `write_document_and_chunks` (document node creation) is independent of `write_entities` (entity node creation), and both must complete before `write_chunk_mentions` can run (which MATCHes on both Chunk and Entity nodes).

**Recommendation:**

```rust
// Stage 1: can run in parallel
tokio::join!(
    write_document_and_chunks(neo4j, cfg, &url, &source_type, &chunks),
    write_entities(neo4j, &entities),
    write_entity_relationships(neo4j, &relationships),
);

// Stage 2: depends on Stage 1 completing
let mention_count = write_chunk_mentions(...).await?;
```

This matches the dependency graph: `MENTIONED_IN` edges (`write_chunk_mentions`) require `Entity` and `Chunk` nodes to exist, which are created in `write_entities` and `write_document_and_chunks` respectively.

---

## 3. Embed Worker Pipeline

### 3.1 Full `Config` Clone to Override One Field

**Severity:** High
**Estimated impact:** Eliminates a ~149-field heap copy per embed job; on a 100-job batch this is 100 Config copies
**File:** `crates/jobs/embed/worker.rs`, lines 120–121

**Finding:**

```rust
// worker.rs:120 — clones the entire 149-field Config struct to change one field
let mut embed_cfg = cfg.clone();
embed_cfg.collection = collection.clone();
let summary_result =
    embed_path_native_with_progress(&embed_cfg, &input_text, Some(progress_tx), source_type)
        .await;
```

The `Config` struct is 527 lines, containing strings, Vecs, Options, PathBufs — many heap-allocated fields. This clone is performed per-job at runtime. The only mutation is `collection`, a single `String`.

**Recommendation:** Pass `collection` as a separate parameter to `embed_path_native_with_progress`, allowing the function to use `&Config` plus a `collection` override without cloning the entire struct. Alternatively, use `Arc<Config>` everywhere and create a thin override layer only for fields that vary per-job.

The broader problem noted in Phase 1 context — Config cloned 20+ times including per-job in `ingest/process.rs` — applies here too. The systemic fix is `Arc<Config>` at the worker boundary (the graph worker already wraps `Config` in `Arc`; embed does not).

---

### 3.2 Progress DB Update on Every Document

**Severity:** Medium
**Estimated impact:** Reduces DB writes from N-per-job to ~N/10 under high-throughput embed jobs
**File:** `crates/jobs/embed/worker.rs`, lines 102–119

**Finding:**

```rust
let progress_task = tokio::spawn(async move {
    while let Some(progress) = progress_rx.recv().await {
        let progress_json = serde_json::json!({ ... });
        let _ = sqlx::query(
            "UPDATE axon_embed_jobs SET updated_at=NOW(), result_json=$2 WHERE id=$1 AND status=$3",
        )
        .bind(id)
        .bind(progress_json)
        .bind(JobStatus::Running.as_str())
        .execute(&progress_pool)
        .await;  // ← one PG write per progress event
    }
});
```

The progress task fires one `UPDATE` per document completion. For a 500-document embed job, this generates 500 Postgres writes. The crawl worker applies a 500ms debounce:

```rust
// process.rs — crawl worker uses a 500ms debounce gate
if last_update.elapsed() < Duration::from_millis(500) {
    continue;
}
```

The embed progress task has no such debounce. Under concurrent embed lanes, this can generate significant write pressure on the `axon_embed_jobs` table.

**Recommendation:** Apply the same 500ms debounce pattern from the crawl worker's `spawn_progress_task`.

---

### 3.3 First-Doc Serial Bootstrap Has Hidden Cost

**Severity:** Medium
**Estimated impact:** 1–3 second latency penalty at job start for single-doc embed jobs
**File:** `crates/vector/ops/tei/pipeline.rs`, lines 331–346

**Finding:**

```rust
// pipeline.rs — Phase 1 processes the first doc SERIALLY to determine VectorMode
let Some(first_doc) = work.next() else { ... };
let (mode, mut chunks_embedded, mut docs_failed) = bootstrap_first_doc(
    cfg, first_doc, doc_timeout_secs, &mut state.pending_points, &mut state.stale_tail_queue,
).await?;
```

This is intentional: the first doc is processed to determine whether the collection is `Named` or `Unnamed` before committing the rest of the batch to a vector format. However, for jobs where `VectorMode` is already known (i.e., the collection mode cache in `COLLECTION_MODES` is populated), the serial bootstrap still happens and blocks the entire batch.

The `collection_init_or_cached()` function checks the cache, but `bootstrap_first_doc` calls it only *after* embedding the first doc (line 165):

```rust
async fn bootstrap_first_doc(...) {
    match embed_prepared_doc_with_timeout(..., VectorMode::Unnamed).await {
        Ok((dim, url, chunk_count, points)) => {
            let mode = qdrant_store::collection_init_or_cached(cfg, dim).await ...;
```

**Recommendation:** Check the `COLLECTION_MODES` cache before entering the serial bootstrap. If the mode is cached, skip the serial phase entirely and go directly to the concurrent drain with the known mode.

---

## 4. Config Clone Bomb

**Severity:** High
**Estimated impact:** Eliminates 100–200 MB/s of heap churn under sustained multi-job workloads
**File:** `crates/core/config/types/config.rs` (527 lines), `crates/jobs/embed/worker.rs:120`

**Finding:**

The `Config` struct is 527 lines with 149+ fields including `Vec<String>` for `exclude_path_prefix` (110+ entries), `ask_authoritative_domains`, `custom_headers`, and multiple `String` fields for URLs. This struct is passed by reference everywhere but cloned at the worker boundary.

Known clone sites:
- `embed/worker.rs:120` — `cfg.clone()` to override `collection`
- The graph worker wraps `Config` in `Arc<Config>` correctly (line 410–411 in `worker.rs`), but the embed worker does not
- MEMORY.md notes "149-field struct cloned 20+ times, including per-job in `ingest/process.rs`"

**Recommendation:** Apply `Arc<Config>` at all worker entry points. The graph worker's pattern is the template:

```rust
// graph/worker.rs — correct pattern
async fn process_claimed_graph_job(
    cfg: std::sync::Arc<Config>,  // Arc, not &Config
    ...
```

The embed `process_claimed_embed_job` already accepts `Arc<Config>` at line 252, but `process_embed_job` at line 233 takes `&Config` and clones it for the collection override. The fix is to pass the collection as a separate parameter rather than cloning Config.

---

## 5. Qdrant Retry Logic — Missing Jitter

**Severity:** High
**Estimated impact:** Eliminates thundering-herd reconnect storms on Qdrant restart; matters at scale
**File:** `crates/vector/ops/qdrant/client.rs`, lines 44, 59, 88, 100

**Finding:**

Both `qdrant_delete_with_retry` and `scroll_page_with_retry` use fixed exponential backoff without jitter:

```rust
// client.rs:44 — qdrant_delete_with_retry
tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;

// client.rs:59 — qdrant_delete_with_retry transport error path
tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;

// client.rs:88 — scroll_page_with_retry
tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;

// client.rs:100 — scroll_page_with_retry transport error path
tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;
```

This is `250ms, 500ms, 1000ms, 2000ms` — clean powers of 2 with no randomization. When Qdrant restarts under load (or returns transient 503s), all concurrent embed/crawl workers retry at exactly the same intervals, amplifying the reconnect storm rather than spreading it.

By contrast, the TEI retry logic in `tei_client.rs:47` correctly adds jitter:

```rust
// tei_client.rs — correct pattern
fn retry_delay(attempt: usize) -> Duration {
    let base_ms = 1000_u64.saturating_mul(2u64.saturating_pow(attempt as u32 - 1));
    let capped_ms = base_ms.min(TEI_MAX_BACKOFF_MS);
    let jitter = Duration::from_millis(rand::rng().random_range(0..500));
    Duration::from_millis(capped_ms) + jitter
}
```

**Recommendation:** Extract the TEI `retry_delay` function to a shared utility and use it in both Qdrant retry paths. The pattern is already proven; it just needs to be applied consistently.

---

## 6. Database Performance

### 6.1 `count_stale_and_pending_jobs` Creates a Pool Per Call

**Severity:** High
**Estimated impact:** Eliminates pool-creation overhead (~50ms) on every health-check / watchdog invocation
**File:** `crates/jobs/common/stats.rs`, lines 56–62

**Finding:**

```rust
// stats.rs — creates a NEW pool on every call
pub async fn count_stale_and_pending_jobs(cfg: &Config, stale_minutes: i64) -> Option<(i64, i64)> {
    let pool = match make_pool(cfg).await {
        Ok(p) => p,
        Err(_) => return None,
    };
    count_stale_and_pending_jobs_with_pool(&pool, stale_minutes).await
}
```

`make_pool()` creates a `PgPool` with connection overhead. The `_with_pool` variant exists and is correct — the problem is callers that use the `_without_pool` variant (which creates a throwaway pool). This pattern should not exist; callers should pass their existing pool.

**Recommendation:** Audit all call sites of `count_stale_and_pending_jobs` (without `_with_pool`) and replace with the pool-reusing variant. Deprecate or remove the non-pool variant.

### 6.2 `pg_pool_for_stats` Creates a Separate 2-Connection Pool for Stats

**Severity:** Medium
**Estimated impact:** Eliminates a parallel 2-connection pool created on every `axon stats` invocation
**File:** `crates/vector/ops/stats/pg.rs`, lines 25–36

**Finding:**

```rust
pub(super) async fn pg_pool_for_stats(cfg: &Config) -> Option<sqlx::PgPool> {
    // Creates a fresh 2-connection pool with a 3-second timeout
    tokio::time::timeout(
        Duration::from_secs(3),
        PgPoolOptions::new().max_connections(2).connect(&cfg.pg_url),
    ).await.ok().and_then(Result::ok)
}
```

Every call to `axon stats` creates a fresh Postgres connection pool. While `max_connections(2)` limits the pool size, creating and destroying pools adds overhead (TCP handshake, SSL negotiation, Postgres auth handshake). For the `stats` command this is called once per invocation, so the cost is bounded — but it's still unnecessary if a pool is already available.

**Recommendation:** Pass `Option<&PgPool>` into `collect_postgres_metrics`. When a pool exists (workers), reuse it. When none exists (CLI invocation), create the short-lived stats pool as a last resort.

### 6.3 Crawl Stats Query Uses Lateral Join Without Index Guarantee

**Severity:** Medium
**Estimated impact:** Can cause multi-second query plans on large `axon_embed_jobs` tables
**File:** `crates/vector/ops/stats/pg.rs`, lines 189–207

**Finding:**

```sql
SELECT AVG(EXTRACT(EPOCH FROM (
    COALESCE(e.finished_at, c.finished_at) - c.started_at
))::double precision)
FROM axon_crawl_jobs c
LEFT JOIN LATERAL (
    SELECT finished_at
    FROM axon_embed_jobs e
    WHERE e.status='completed'
      AND e.input_text LIKE ('%' || c.id::text || '/markdown')
    ORDER BY finished_at DESC
    LIMIT 1
) e ON TRUE
WHERE c.status='completed' AND ...
```

This `LATERAL` join searches `axon_embed_jobs.input_text` with a `LIKE '%<uuid>/markdown'` pattern. A leading `%` wildcard on a `LIKE` query cannot use a standard B-tree index — it forces a sequential scan of `axon_embed_jobs` for every `axon_crawl_jobs` row where `status='completed'`. With thousands of crawl jobs and tens of thousands of embed jobs, this is an O(crawl_count × embed_count) full table scan nested loop.

**Recommendation:** Either:
1. Add a `crawl_job_id` foreign key column to `axon_embed_jobs` and index it, replacing the fragile `input_text LIKE '%uuid%'` pattern; or
2. Drop this particular metric from `stats` (average overall crawl + embed duration) as it's a derived metric of questionable operational value

Option 1 requires a schema migration. Option 2 is the lower-risk path given the fragility of the LIKE pattern (it would silently break if `input_text` format changes).

### 6.4 `qdrant_urls_for_domain` Performs Full Scroll for Domain Detail

**Severity:** Medium
**Estimated impact:** 10–60 second latency on large collections for detailed domain views
**File:** `crates/vector/ops/qdrant/client.rs`, lines 239–247

**Finding:**

```rust
pub(crate) async fn qdrant_urls_for_domain(cfg: &Config, domain: &str) -> Result<HashSet<String>> {
    let filter = serde_json::json!({
        "must": [
            {"key": "domain", "match": {"value": domain}},
            {"key": "chunk_index", "match": {"value": 0}}
        ]
    });
    scroll_url_set(cfg, filter, None).await  // None = no limit = full scan
}
```

This is called from `qdrant_delete_stale_domain_urls` (maintenance) and potentially the detailed domains view. With a 7M point collection, scrolling all chunk_index=0 points for even a mid-sized domain could touch hundreds of thousands of points.

**Recommendation:** Use Qdrant's `/facet` endpoint filtered by `domain` to count/list URLs for a domain in O(1), consistent with `qdrant_url_facets` already in `client.rs`. The facet endpoint supports filters since Qdrant 1.9+.

---

## 7. Memory Management

### 7.1 `detailed_domains` Aggregates On-the-Fly but `HashSet<String>` Grows Unbounded

**Severity:** Medium
**Estimated impact:** Caps RSS growth during large domain scans; prevents OOM on 7M-point collections
**File:** `crates/services/system.rs`, lines 180–229

**Finding:**

The `detailed_domains` function correctly aggregates on-the-fly inside the scroll callback rather than buffering all payloads:

```rust
// system.rs:195 — streaming aggregation (correct)
qdrant_scroll_pages_selective(cfg, serde_json::json!({"include": ["domain", "url"]}), |points| {
    for point in points {
        let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
        entry.0 += 1;
        if !url.is_empty() {
            entry.1.insert(url);  // ← HashSet<String> grows without bound
        }
        ...
```

However, `entry.1` is a `HashSet<String>` that accumulates every unique URL per domain. With 7M points and the cortex collection containing 400,000+ URLs, the `by_domain` HashMap could accumulate millions of unique URL strings in memory simultaneously.

The `DEFAULT_DOMAINS_DETAILED_LIMIT` of 10,000,000 means this can process the entire 7M-point collection. Each URL string averages ~60 bytes; 400,000 URLs = ~24 MB just for URL strings in the HashSet. The HashMap overhead (HashSet buckets, domain strings) adds more.

**Recommendation:** The `urls` field in `DetailedDomainFacet` is only a count (`urls.len()`). The HashSet is used solely to deduplicate URLs. Replace `HashSet<String>` with a `HashSet<u64>` by hashing each URL with `std::hash::DefaultHasher` or FNV, storing only the hash. This reduces per-URL memory from ~60+ bytes to 8 bytes — a 7–8x reduction in the URL dedup structure.

### 7.2 `sorted_urls` Clones All Strings from HashSet

**Severity:** Low
**Estimated impact:** Minor allocation savings; bounded by the size of thin/WAF-blocked URL sets
**File:** `crates/jobs/crawl/runtime/worker/process.rs`, lines 159–163

**Finding:**

```rust
fn sorted_urls(values: &HashSet<String>) -> Vec<String> {
    let mut urls: Vec<String> = values.iter().cloned().collect();
    urls.sort();
    urls
}
```

This clones every `String` in the HashSet. For the typical case (cancellation with partial results), the sets are small. But for large crawls that hit many thin pages, `summary.thin_urls` could be tens of thousands of entries.

For write-once JSON serialization, borrowing is sufficient. The function could return `Vec<&str>` to avoid cloning:

```rust
fn sorted_urls(values: &HashSet<String>) -> Vec<&str> {
    let mut urls: Vec<&str> = values.iter().map(String::as_str).collect();
    urls.sort_unstable();
    urls
}
```

However, `serde_json::json!` macro requires `Serialize` — `Vec<&str>` serializes correctly. The change is safe if the lifetime of `values` outlives the JSON value construction.

---

## 8. Concurrency & Async Efficiency

### 8.1 `full_status` Loads 6 × 20 Job Records on Every Status Poll

**Severity:** Medium
**Estimated impact:** Reduces status query latency from ~60ms (6 sequential) to ~20ms (parallel already); reduces result size
**File:** `crates/services/system.rs`, lines 297–329

**Finding:**

`load_status_jobs` uses `tokio::join!` to parallelize 6 DB queries — this is correct. Each query fetches up to 20 jobs. However, the `filter_and_view` pass then discards watchdog-reclaimed failures. This means the Postgres queries always fetch 20 rows per table even when most will be filtered out.

More importantly, the `list_*_jobs` functions likely perform a full `SELECT` without an efficient status-filtered index. If the job tables have thousands of rows, `LIMIT 20` without an index on `(status, created_at DESC)` causes a full table scan.

**Recommendation:** Verify that `axon_crawl_jobs`, `axon_embed_jobs`, etc. have composite indexes on `(status, created_at DESC)`. The query pattern is always "most recent N jobs" optionally filtered by status — without this index, each query is O(total_jobs) even with LIMIT.

### 8.2 `FuturesUnordered` in Embed Pipeline Has Correct Sliding Window

**Severity:** None (existing good pattern)
**File:** `crates/vector/ops/tei/pipeline.rs`, lines 240–289

The `drain_concurrent_docs` function correctly implements a sliding window: it fills `FuturesUnordered` up to `doc_concurrency`, then as each completes, adds the next doc. This avoids materializing all futures at once. No action needed.

### 8.3 `check_embed_canceled` Opens Redis Connection Per Job, Not Per Worker

**Severity:** High
**Estimated impact:** Eliminates 50–200ms Redis connection overhead per embed job
**File:** `crates/jobs/embed/worker.rs`, lines 15–42, 157

**Finding:**

```rust
// worker.rs:157 — called in process_embed_job_with_runner, once per job
let mut redis_conn = open_embed_redis(cfg).await;
```

`open_embed_redis` opens a new Redis `MultiplexedConnection` per embed job. This involves a TCP connection, authentication, and Redis HELLO handshake. With `AXON_EMBED_LANES=2` and many small jobs (e.g., individual URL re-embeds), this overhead is significant relative to job processing time.

**Recommendation:** Open the Redis connection once at worker startup (in `run_embed_worker`), wrap it in an `Arc<Mutex<MultiplexedConnection>>`, and pass it to `process_claimed_embed_job`. The `MultiplexedConnection` can handle multiple concurrent requests by design.

Note: The crawl worker handles cancel via a polling loop with a shared cancel key check — a simpler pattern that doesn't require per-job connection setup.

---

## 9. Crawl Worker Pipeline

### 9.1 `AXON_CRAWL_SIZE_WARN_THRESHOLD` Env Var Read Per Crawl Job

**Severity:** Low
**Estimated impact:** Eliminates one `env::var()` call per completed crawl; negligible in absolute terms
**File:** `crates/jobs/crawl/runtime/worker/process.rs`, lines 400–413

**Finding:**

```rust
let size_warn_threshold: u32 = std::env::var("AXON_CRAWL_SIZE_WARN_THRESHOLD")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(10_000);
```

This is inside `run_active_crawl_job`, called after every crawl completes. The comment states "reads on each job so it can be tuned without restarting the worker." This is a deliberate trade-off for operator convenience (hot-reload). Given the low call frequency (once per crawl, not per page), this is acceptable. Document the intent in the code comment to prevent future "cleanup" that removes the intentional runtime read.

### 9.2 `spawn_progress_task` Debounce Is Correct

**Severity:** None (existing good pattern)
**File:** `crates/jobs/crawl/runtime/worker/process.rs`, lines 246–250

The crawl worker's progress task correctly applies a 500ms debounce:

```rust
if last_update.elapsed() < Duration::from_millis(500) {
    continue; // drain channel, skip DB write
}
```

This is the pattern the embed worker should adopt (see §3.2).

---

## 10. Scalability Barriers

### 10.1 `COLLECTION_MODES` Cache Cannot be Invalidated Without Process Restart

**Severity:** Medium
**Estimated impact:** Silent correctness bug during collection migration; not a throughput issue
**File:** `crates/vector/ops/tei/qdrant_store.rs`, lines 28–66

**Finding:**

```rust
/// Process-lifetime cache: entries are never evicted. A process restart is required
/// to pick up collection schema changes (e.g., migration from Unnamed to Named).
static COLLECTION_MODES: OnceLock<RwLock<HashMap<String, VectorMode>>> = OnceLock::new();
```

The `clear_collection_mode_cache` function exists but is `#[expect(dead_code)]`. During an `axon migrate` operation (Unnamed → Named), running workers will continue operating in `VectorMode::Unnamed` mode for the duration of their process lifetime, writing wrong-format points to the now-Named collection.

**Recommendation:** Either:
1. Add a Redis-backed cache invalidation signal that workers poll, clearing the local cache; or
2. Document the required worker restart clearly in `axon migrate` output and the CLAUDE.md

The documentation in CLAUDE.md mentions "process restart required" but this warning is not surfaced in the CLI output.

### 10.2 Single AMQP Consumer Per Lane — No Work Stealing

**Severity:** Medium
**Estimated impact:** Lane starvation when one job is much longer than others; not a correctness bug
**File:** `crates/jobs/worker_lane.rs` (not read but referenced in CLAUDE.md)

Each worker lane holds exactly one AMQP consumer and processes one job at a time within the lane. If one embed job takes 300 seconds (large corpus) and the other lane processes 50 small jobs in the same window, AMQP's consumer_timeout may trigger on the idle lane while the large job is running.

**Recommendation:** Implement `consumer_timeout` keepalives (nack + requeue) for jobs that exceed a threshold, or document the consumer_timeout setting required to accommodate large jobs.

### 10.3 Neo4j Write Contention Under `graph_concurrency > 1`

**Severity:** Medium
**Estimated impact:** Deadlocks or lock waits on `Entity` and `Document` nodes with high concurrency
**File:** `crates/jobs/graph/worker.rs`, lines 211–239

**Finding:**

`write_entities` issues `UNWIND ... MERGE (e:Entity {name: ...}) SET ...`. Multiple concurrent graph workers processing different URLs that share entities (e.g., "Tokio", "Docker") will contest the same `Entity` nodes in Neo4j. Neo4j handles MERGE under concurrent transactions with an "eager" lock, but at `graph_concurrency=4+` with many shared entities (technology taxonomy), this can serialize.

**Recommendation:** Neo4j 5.x supports `CALL { ... } IN TRANSACTIONS OF N ROWS` for batched MERGE with controlled transaction size. Profile Neo4j lock wait time under `graph_concurrency=4`. If waits are observed, reduce concurrency for entity writes to a dedicated serial lane while keeping similarity computation (Qdrant, read-only) at full concurrency.

---

## 11. Summary Priority Table

| # | Finding | Severity | File | Estimated Impact |
|---|---------|----------|------|-----------------|
| 1.1 | `env_usize_clamped` called per batch in `tei_embed()` | **High** | `tei/tei_client.rs:200–203` | Eliminates 3× env mutex per batch |
| 2.1 | N+1 Qdrant calls: one scroll per graph job URL | **Critical** | `graph/worker.rs:319` | 1000× reduction in Qdrant RTTs for batch graph builds |
| 2.4 | Sequential Neo4j writes in `process_graph_job` | **High** | `graph/worker.rs:385–391` | 30–50% per-job latency reduction |
| 3.1 | `cfg.clone()` per embed job to change one field | **High** | `embed/worker.rs:120` | Eliminates 149-field heap copy per job |
| 4 | Config clone bomb (systemic) | **High** | Multiple | RSS reduction under sustained load |
| 5 | Qdrant retry without jitter | **High** | `qdrant/client.rs:44,59,88,100` | Eliminates thundering-herd on Qdrant restart |
| 6.1 | `count_stale_and_pending_jobs` creates pool per call | **High** | `jobs/common/stats.rs:56–62` | Eliminates ~50ms pool creation overhead |
| 8.3 | Redis connection opened per embed job | **High** | `embed/worker.rs:15–42,157` | Eliminates 50–200ms per job Redis handshake |
| 1.2 | TEI semaphore default too low for multi-lane workloads | **Medium** | `tei/tei_client.rs:41` | 10–30% throughput increase |
| 2.2 | Unnecessary `String` clones in `group_by_url_max_score` | **Medium** | `graph/similarity.rs:49–69` | O(n) allocation reduction in similarity grouping |
| 3.2 | Embed progress task: no debounce, one DB write per doc | **Medium** | `embed/worker.rs:102–119` | Reduces DB write pressure under high-volume embed |
| 3.3 | Serial first-doc bootstrap even when VectorMode is cached | **Medium** | `tei/pipeline.rs:331–346` | Eliminates 1–3s pipeline latency when mode cached |
| 6.3 | Lateral JOIN with leading `%LIKE%` in crawl stats | **Medium** | `stats/pg.rs:189–207` | Prevents O(n²) query plan on large job tables |
| 6.4 | `qdrant_urls_for_domain` does full scroll (no limit) | **Medium** | `qdrant/client.rs:239–247` | Replaces minutes-long scroll with O(1) facet |
| 7.1 | `HashSet<String>` URL dedup stores full URLs in RAM | **Medium** | `services/system.rs:190–218` | 7–8x RSS reduction for detailed domain scan |
| 8.1 | Status queries may lack composite index on status+time | **Medium** | `services/system.rs:297–329` | Prevents O(n) table scan on job tables |
| 10.1 | `COLLECTION_MODES` cache uncleared during migration | **Medium** | `tei/qdrant_store.rs:28–66` | Correctness: prevents wrong-format point writes |
| 10.3 | Neo4j MERGE contention under `graph_concurrency > 1` | **Medium** | `graph/worker.rs:211–239` | Prevents lock wait serialization on shared entities |
| 6.2 | `pg_pool_for_stats` creates separate pool per stats call | **Medium** | `stats/pg.rs:25–36` | Eliminates pool creation on every `axon stats` |
| 2.3 | Endpoint URL string rebuilt per `compute_similarity` call | **Low** | `graph/similarity.rs:78–82` | One allocation per graph job; cosmetic |
| 7.2 | `sorted_urls` clones all strings from HashSet | **Low** | `crawl/worker/process.rs:159–163` | Minor; bounded by thin/WAF URL set size |
| 9.1 | `AXON_CRAWL_SIZE_WARN_THRESHOLD` read per crawl | **Low** | `crawl/worker/process.rs:400` | Intentional hot-reload trade-off; document intent |
| 10.2 | No work stealing: single AMQP consumer per lane | **Medium** | `jobs/worker_lane.rs` | Lane starvation risk under mixed job sizes |

---

## Key Systemic Patterns

**Already correct — do not regress:**

1. `LazyLock` for HNSW search params (`HNSW_EF_SEARCH`, `HNSW_EF_SEARCH_LEGACY`) — correct, extend to TEI params
2. TEI retry jitter (`retry_delay` in `tei_client.rs`) — correct, apply to Qdrant retries
3. `FuturesUnordered` sliding window in `drain_concurrent_docs` — correct, no changes needed
4. Facet API for `sources` and `domains` aggregation — correct, extend to per-domain URL counts
5. Streaming aggregation in `detailed_domains` — correct, but URL dedup can be memory-optimized
6. `tokio::join!` parallelism in `load_status_jobs` and all stats collectors — correct
7. `VectorMode` cache with `RwLock` allowing concurrent reads — correct
8. Graph worker uses `Arc<Config>` — correct, extend this pattern to embed worker

**The three highest-leverage fixes in order:**

1. **Graph N+1 (§2.1)** — Eliminate one Qdrant scroll per graph job URL; convert to batch retrieval. For a 1,000-URL graph build, this changes the dominant cost from network I/O to a single batch query.

2. **Qdrant retry jitter (§5)** — Apply `retry_delay()` with jitter to all Qdrant retry paths. Four identical `250ms * 2^n` backoff sites are currently live; a Qdrant restart will generate a synchronized retry storm from all workers.

3. **Config clone (§4 / §3.1)** — Replace `cfg.clone()` in `embed/worker.rs:120` with a collection parameter. Extend `Arc<Config>` to the embed worker boundary. The graph worker already shows the correct pattern.
