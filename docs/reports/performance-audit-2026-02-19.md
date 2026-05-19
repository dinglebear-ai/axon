# Performance & Memory Audit — axon_rust
**Date:** 2026-02-19
**Auditors:** 4 parallel agents (Jobs/AMQP, Vector/Qdrant, Crawl Engine, Core Utilities)
**Branch:** `perf/command-performance-fixes`

---

## Executive Summary

66 total findings across 4 subsystems. The codebase has **3 systemic antipatterns** that each appear in multiple subsystems and drive most of the performance cost:

1. **Connection-per-call** — HTTP clients, DB pools, Redis connections, and AMQP channels created fresh per invocation rather than shared across calls.
2. **Unbounded channel backpressure** — `unbounded_channel()` for progress reporting lets producers race ahead of DB-writing consumers; memory grows proportionally to Postgres latency × crawl throughput.
3. **Full-collection accumulation defeating streaming** — `qdrant_scroll_pages` was correctly designed for streaming, but two consumers (`dedupe`, `domains`) immediately re-accumulate everything into `HashMap`s, loading the full collection into heap.

Fix these three patterns and the secondary findings shrink significantly.

---

## Priority Matrix

| Priority | Finding | Subsystem | Impact |
|----------|---------|-----------|--------|
| **P0** | `make_pool()` per CLI function call — new PgPool on every `get_*`/`list_*`/`cancel_*` | Jobs | Postgres connection exhaustion under concurrency |
| **P0** | `enqueue_job` opens full AMQP TCP connection per call | Jobs | 6–8 round-trips per enqueue; stalls callers for up to 5s under load |
| **P0** | `unbounded_channel` for crawl/embed progress | Jobs × 2 | Memory grows unbounded when Postgres write lags behind crawler output |
| **P0** | `run_dedupe_native` loads entire collection into HashMap | Vector | OOM on collections > 100k points (~100 MB per million points) |
| **P0** | `run_domains_native` (detailed mode) loads all URLs into HashSet | Vector | OOM on large collections |
| **P1** | Broadcast channel drops pages silently on lag (`RecvError::Lagged`) | Crawl | Silent data loss under `extreme`/`max` profile crawls |
| **P1** | `build_client()` per command invocation (batch.rs, search.rs) | Core | TLS context + connection pool torn down after single use |
| **P1** | Redis client created per job execution (embed, extract, crawl workers) | Jobs × 3 | TCP handshake per job; extra failure point per job |
| **P1** | `is_excluded_url_path` re-normalises already-normalised prefixes per URL | Core | 15,000+ `String` allocations per 500-page crawl |
| **P1** | Panic in job processing leaves job stuck in `running` (zombie) | Jobs | 300s stall per panic until watchdog reclaims |
| **P2** | `build_transform_config()` called per page in `to_markdown()` and sitemap backfill | Core/Crawl | Struct rebuilt on every page transform; fix is one `LazyLock` |
| **P2** | `qdrant_base()` allocates new `String` per call | Vector/Core | Called in every qdrant HTTP path; trivial to return `&str` |
| **P2** | AMQP probe channels opened and silently dropped without close handshake | Jobs × 4 | Orphaned RabbitMQ channels accumulate under crash-loop restarts |
| **P2** | `Vec<usize>` char-index table allocated per `chunk_text()` call | Vector/Core | 800 KB allocation for 100KB documents; avoidable with fast-path |
| **P2** | `Vec<char>` in `clean_inline_markdown` + `split_into_sentences` | Vector | 8 KB allocation per call for snippet processing |
| **P2** | `pending_points` buffer threshold 2048 — holds 11 MB before flush | Vector | Default too high; Qdrant handles 256 point batches efficiently |
| **P2** | Full HTML/XML cloned to do case-insensitive tag search | Core × 2 | 50–500 KB clone for `extract_meta_description`; 1–5 MB for `extract_loc_values` |
| **P2** | TEI embed URL reconstructed inside retry loop | Core | Hoist `format!` above loop; trivial 2-line change |
| **P3** | `chars().count()` is O(n) where `.len()` suffices for thin-page threshold | Crawl/Vector | Replace with O(1) byte count |
| **P3** | Full sitemap URL Vec materialized then re-filtered into second Vec | Crawl | Two copies of potentially 10k+ URL strings |
| **P3** | Auto-switch: HTTP and Chrome result sets coexist in memory during retry | Crawl | ~40 KB for 500 URLs; bounded but fixable |
| **P3** | `build_transform_config()` not reused across sitemap backfill pages | Crawl | Per-page struct rebuild in `to_markdown()` |
| **P3** | `try_auto_switch` clones URL Vec on no-switch path | Crawl | Accept `Vec<String>` by value; return directly |
| **P3** | Two separate `reqwest::Client` instances in sitemap pipeline | Crawl | Both pools idle while the other runs |
| **P3** | `Arc::new(cfg.clone())` inside `fetch_full_docs` | Vector | Full Config clone; reference would suffice |
| **P3** | SSE streaming buffer uses `drain` in tight loop (O(lines×remaining) work) | Vector | Use read cursor instead |
| **P3** | `rerank_ask_candidates` clones entire candidate slice | Vector | Common empty-tokens path clones and returns unchanged |
| **P3** | `IS_DOCKER` filesystem stat (`stat("/.dockerenv")`) called at runtime | Core | Cache in `LazyLock<bool>` |
| **P3** | `SCHEMA_INITIALIZED: OnceLock` declared but never wired | Jobs | DDL round-trip on every `get_*`/`list_*` call |
| **P4** | Mutex lock on every crawl progress tick for injection state | Jobs | Use `OnceLock` — value set exactly once |
| **P4** | Sequential HTTP fetch in batch worker (ignores `batch_concurrency` config) | Jobs | 500ms × 50 URLs = 25s; concurrent = 1.6s |
| **P4** | `qdrant_base()` defined identically in 3 modules | Vector | DRY violation |
| **P4** | `Style::new()` constructed and discarded per terminal output call | Core | Cache styles in `LazyLock` |
| **P4** | Logging callers pay `format!()` cost regardless of log level | Core | Anti-pattern; use `tracing!` macros directly at call sites |
| **P4** | Double clamp on TEI `batch_size` silently caps env var at 128 | Vector | Confusing; fix the `env_usize_clamped` upper bound |

---

## Detailed Findings by Subsystem

### 1. Jobs / AMQP

#### P0 — `make_pool()` per CLI function call (F-01)
**Files:** `batch_jobs.rs`, `embed_jobs.rs`, `extract_jobs.rs`, `crawl_jobs/runtime/mod.rs`
Every public function (`get_*`, `list_*`, `cancel_*`, `cleanup_*`, `start_*`) calls `make_pool(cfg).await?` independently, creating a brand-new `PgPool` on every invocation. Each pool holds up to 5 connections. Under concurrent CLI calls + running workers, Postgres receives dozens of simultaneous connection requests and can hit `max_connections`.

**Fix:** Create the pool once in the command dispatch layer and pass `&PgPool` into all subordinate functions. Worker paths already do this correctly — apply the same pattern to CLI-facing paths.

---

#### P0 — Full AMQP TCP connection per `enqueue_job` (F-18)
**File:** `common.rs:245–270`
Every `enqueue_job` opens a fresh TCP connection, negotiates AMQP, creates a channel, declares the queue, publishes, then closes — 6–8 round trips. Under `apply_queue_injection` this multiplies across 12–24 URLs.

**Fix:** For high-frequency enqueueing paths, collect all job IDs first (DB inserts), then publish all messages on a single channel. Maintain a long-lived channel in workers for outbound publishes, reconnecting only on error.

---

#### P0 — `unbounded_channel` for crawl/embed progress (F-02, F-03)
**Files:** `embed_jobs.rs:218`, `crawl_jobs/runtime/worker/worker_process/execution.rs:48`
Both progress channels are `unbounded`. For a 5,000-page crawl where the DB write takes 10ms and the crawler produces 50 pages/sec, ~500 `CrawlSummary` messages queue in-flight. Each `CrawlSummary` includes a `HashSet<String>` of seen URLs.

**Fix:** Replace `unbounded_channel` with `channel(1)`. Use `try_send` from the hot crawl loop — dropping a progress tick is acceptable (telemetry, not data). Do not `send().await` into a bounded channel from the crawler; that stalls the crawl waiting for a DB write.

---

#### P1 — Panic → zombie job (F-16)
**Files:** All worker processing paths
If `process_*_job` panics, the job stays in `running` status. `mark_job_failed` is only called on `Result::Err` paths — a Rust panic bypasses them. The watchdog reclaims these after `watchdog_stale_timeout_secs` (default 300s).

**Fix:**
```rust
let result = tokio::spawn(process_job(cfg, pool, job_id)).await;
match result {
    Ok(Ok(())) => {}
    Ok(Err(e)) => mark_job_failed(pool, TABLE, job_id, &e.to_string()).await,
    Err(_panic) => mark_job_failed(pool, TABLE, job_id, "worker panic").await,
}
```

---

#### P1 — Redis client created per job execution (F-08, F-09, F-10)
**Files:** `embed_jobs.rs:202`, `extract_jobs/worker.rs:64`, `crawl_jobs/runtime/worker/worker_process/context.rs:38`
`redis::Client::open()` + `get_multiplexed_async_connection()` per job = one TCP handshake per job. The batch worker already does this correctly (one connection at startup, passed through). The embed and extract workers do not.

**Fix:** Create `MultiplexedConnection` once at worker startup. Pass by clone (it's an `Arc` handle) into processing functions.

---

#### P2 — AMQP probe channel leaks (F-04, F-05, F-06, F-07)
**Files:** `embed_jobs.rs:412`, `extract_jobs.rs:346`, `crawl_jobs/runtime/worker/worker_loops.rs:345`, `batch_jobs/worker.rs:257`
The probe uses `.is_ok()`, discarding the `Channel`. Lapin does not issue a graceful `Channel.Close` on drop — RabbitMQ holds the orphaned channel until its heartbeat timeout (60s).

**Fix:**
```rust
let probe_ok = match open_amqp_connection_and_channel(cfg, queue).await {
    Ok((conn, ch)) => { let _ = ch.close(0, "").await; let _ = conn.close(200, "").await; true }
    Err(_) => false,
};
```

---

#### P4 — Sequential HTTP fetch in batch worker (F-11)
**File:** `batch_jobs/worker.rs:48–69`
`fetch_batch_results` iterates URLs with `await` inside a `for` loop. The `batch_concurrency: usize` config field (default: 16) is never used here. 50 URLs × 500ms = 25s sequential vs ~1.6s concurrent.

**Fix:** Use `futures::stream::iter(urls).map(|url| fetch_html(&client, url)).buffer_unordered(cfg.batch_concurrency)`.

---

### 2. Vector / Qdrant

#### P0 — `run_dedupe_native` accumulates entire collection in memory (F-11)
**File:** `crates/vector/ops/qdrant/commands.rs:196–231`
Despite using `qdrant_scroll_pages` (streaming), the callback immediately re-accumulates all data into `HashMap<(url, chunk_index), Vec<DedupeRecord>>`. At 1M points × ~100 bytes per record = ~100 MB heap before dedup logic runs.

**Fix:** Keep only the two most-recent records per key. When inserting a third, immediately mark the oldest for deletion without accumulating. This makes memory proportional to unique `(url, chunk_index)` pairs, not total points.

---

#### P0 — `run_domains_native` detailed mode loads all URLs into HashSet (F-12)
**File:** `crates/vector/ops/qdrant/commands.rs:172–188`
The detailed path (`AXON_DOMAINS_DETAILED=1`) stores every unique URL per domain in `HashMap<String, (usize, HashSet<String>)>`. 500k URLs × 60 bytes = ~30 MB in strings alone.

**Fix:** The fast path (facet queries) already exists and is correct. For the detailed path, add a point-count guard that refuses to run above a threshold, or replace URL accumulation with unique-count approximation via HyperLogLog or cardinality estimation.

---

#### P2 — `pending_points` buffer default 2048 holds ~11 MB (F-06)
**File:** `crates/vector/ops/tei.rs:282`
2048 points × (2KB chunk text + 3.5KB vector serialized) ≈ 11 MB in `pending_points` before flush. Qdrant handles 256 point batches efficiently; `qdrant_upsert` already chunks internally at 256 anyway.

**Fix:** Lower `AXON_QDRANT_POINT_BUFFER` default to 256 to match `AXON_QDRANT_UPSERT_BATCH_SIZE`. The two-level buffer adds memory without throughput benefit.

---

#### P2 — `Vec<usize>` char-index table per `chunk_text()` (F-07)
**File:** `crates/vector/ops/input.rs:8`
For a 100KB document: `Vec<usize>` of 100k offsets = 800 KB. The early-exit path (text fits in one chunk, the common case) still collects the full offset table before checking.

**Fix:**
```rust
// Fast path: no allocation
let char_count = text.chars().count();
if char_count <= MAX {
    return vec![text.to_string()];
}
// Only collect offsets when chunking is actually needed
let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
```

---

#### P2 — `Vec<char>` in snippet utilities (F-08, F-09)
**File:** `crates/vector/ops/qdrant/utils.rs:74, 225`
`clean_inline_markdown` and `split_into_sentences` both do `text.chars().collect::<Vec<char>>()` for indexed access. 2000-char chunk = 8 KB allocation per call.

**Fix:** Rewrite using `char_indices()` iterator with a byte-position cursor. `[`, `]`, `(`, `)` are ASCII — the markdown link parser can scan byte-by-byte without `Vec<char>`.

---

#### P2 — `qdrant_base()` returns new `String` per call (F-17)
**Files:** `tei.rs:45`, `stats/mod.rs:81`, `qdrant/client.rs:9`
Same function defined three times, each allocating a new `String` for a URL that almost never has a trailing slash.

**Fix:** Consolidate to `qdrant/utils.rs` and return `&str`:
```rust
pub fn qdrant_base(cfg: &Config) -> &str {
    cfg.qdrant_url.trim_end_matches('/')
}
```
Zero-copy. All `format!()` call sites handle `&str` cleanly.

---

#### P3 — SSE drain in tight loop (F-18)
**File:** `crates/vector/ops/commands/streaming.rs:120–123`
`pending.drain(..=newline_idx)` shifts all remaining bytes left on every line. For chunks with 20+ lines this is O(lines × remaining).

**Fix:** Track a read cursor; single `drain(..cursor)` after processing all complete lines in a chunk.

---

### 3. Crawl Engine

#### P1 — Broadcast channel drops pages silently under lag (Finding 2)
**File:** `crates/crawl/engine.rs:248, 346`
Spider's broadcast ring buffer is 4096 slots. The consumer runs CPU-heavy `transform_content_input` per page. If the spider produces faster than the consumer transforms, old messages are overwritten. `RecvError::Lagged` is silently `continue`d — pages are dropped without any counter or warning. This is a **data loss** issue, not a memory issue.

**Fix:** Dynamic buffer sizing: `website.subscribe(cfg.max_pages.max(4096) as usize)`. Or switch to bounded `mpsc` with backpressure on the producer side.

---

#### P2 — `chars().count()` is O(n) for thin-page threshold (Finding 9)
**Files:** `engine.rs:287, 393`, `engine/sitemap.rs:237`
The thin-page threshold (`min_markdown_chars`, default 200) is a content-size heuristic. `.chars().count()` walks every byte for Unicode scalar count; `.len()` returns byte count in O(1). For ASCII-dominated markdown they are identical.

**Fix:** Replace `markdown.trim().chars().count()` with `markdown.trim().len()` in all three locations.

---

#### P2 — Double string allocation per page in `run_crawl_once` (Finding 3)
**File:** `crates/crawl/engine.rs:392–394`
`transform_content_input` returns `String`. `.trim().to_string()` allocates a second `String`. Both coexist until `tokio::fs::write` completes.

**Fix:**
```rust
let markdown = transform_content_input(input, &transform_cfg);
let trimmed = markdown.trim(); // &str borrow, no allocation
let chars = trimmed.len();     // O(1)
tokio::fs::write(&path, trimmed.as_bytes()).await?;
```

---

#### P2 — `build_transform_config()` rebuilt per page in sitemap backfill (Finding 12)
**File:** `crates/crawl/engine/sitemap.rs:236–237`
`to_markdown()` calls `build_transform_config()` inside `handle_backfill_result`, which is invoked per URL. `run_crawl_once` already correctly calls it once outside the loop. The sitemap backfill misses this optimization.

**Fix:** Pass a `&TransformConfig` parameter to `handle_backfill_result` and call `transform_content_input` directly, matching `run_crawl_once`'s pattern.

---

#### P3 — `try_auto_switch` clones URL Vec on no-switch path (Finding 4)
**File:** `crates/crawl/engine.rs:471–495`
Both early-return paths call `.to_vec()` on the input `&[String]`, cloning all URL strings even when no switch is needed.

**Fix:** Change signature to accept `urls: Vec<String>` (owned). Return `urls` directly on no-switch paths.

---

### 4. Core HTTP / Content

#### P1 — `build_client()` per command invocation (Findings 1, 2)
**Files:** `crates/cli/commands/batch.rs:325`, `crates/cli/commands/search.rs:24`
`build_client(20)` constructs a new TLS context and connection pool on every `run_batch_sync` and `run_search` invocation. The client is created, used for one command, then dropped — all TLS session cache benefit is lost.

**Fix:** Use `http_client()?` (the process-global `LazyLock<Client>`). If the 20s timeout is required, add a second `LazyLock<Client>` with that timeout, initialized once.

---

#### P1 — `is_excluded_url_path` re-normalises already-normalised prefixes (Finding 7)
**File:** `crates/core/content.rs:166–175`
`filter_map(|p| normalize_prefix(p))` allocates a new `String` per prefix per URL checked. On a 500-page crawl with 30 default prefixes: 15,000+ allocations. The prefixes are already normalised by `normalize_exclude_prefixes()` at parse time.

**Fix:** Remove the `filter_map` wrapper:
```rust
prefixes.iter().any(|p| path == p.as_str() ||
    (path.starts_with(p.as_str()) && path.as_bytes().get(p.len()) == Some(&b'/')))
```

---

#### P2 — `build_transform_config()` per `to_markdown()` call (Finding 3)
**File:** `crates/core/content.rs:39–51`
Called from 9+ locations including the per-page crawl broadcast loop. The config is always identical.

**Fix:**
```rust
static TRANSFORM_CFG: LazyLock<TransformConfig> = LazyLock::new(build_transform_config);

pub fn to_markdown(html: &str) -> String {
    let input = TransformInput { html: html.to_owned(), url: String::new() };
    transform_content_input(input, &TRANSFORM_CFG).trim().to_string()
}
```

---

#### P2 — Full HTML/XML cloned for case-insensitive tag search (Findings 5, 6)
**Files:** `crates/core/content.rs:99, 131–133`
- `extract_meta_description`: clones entire HTML (50–500 KB) via `to_ascii_lowercase()` just to find `name="description"` — always lowercase in practice.
- `extract_loc_values`: clones entire sitemap XML (1–5 MB) via `to_ascii_lowercase()` just to find `<loc>` — mandatory lowercase in sitemap spec.

**Fix for `extract_meta_description`:** Limit search to `<head>` (at most 8 KB):
```rust
let head_end = html.find("</head>").unwrap_or(html.len().min(8192));
let lower = html[..head_end].to_ascii_lowercase();
```

**Fix for `extract_loc_values`:** Remove the lowercase entirely — sitemap spec mandates `<loc>`. Search `xml` directly.

---

#### P2 — TEI embed URL reconstructed inside retry loop (Finding 10, 11)
**File:** `crates/vector/ops/tei.rs:63–68`
`format!("{}/embed", cfg.tei_url.trim_end_matches('/'))` runs on every retry iteration.

**Fix:** Hoist above the loop:
```rust
let embed_url = format!("{}/embed", cfg.tei_url.trim_end_matches('/'));
while let Some(chunk) = stack.pop() {
    let resp = client.post(&embed_url)...
```

---

#### P3 — `IS_DOCKER` filesystem stat at runtime (Finding 12)
**File:** `crates/core/config/parse.rs:9`
`Path::new("/.dockerenv").exists()` is a `stat` syscall called at startup (acceptable) but also from `health.rs:58` at runtime via `webdriver_url_from_env()`.

**Fix:**
```rust
static IS_DOCKER: LazyLock<bool> = LazyLock::new(|| Path::new("/.dockerenv").exists());
```

---

## Fix Grouping by Effort

### One-line changes (do these first)
- `chars().count()` → `.len()` in 3 locations
- `qdrant_base()` return type `String` → `&str`
- TEI embed URL hoisted above retry loop
- `IS_DOCKER` LazyLock

### Small refactors (< 20 lines each)
- `build_transform_config()` → `LazyLock<TransformConfig>` in `content.rs`
- `extract_loc_values`: remove `to_ascii_lowercase()` (sitemap spec guarantees lowercase)
- `extract_meta_description`: scope lowercase clone to `<head>` only
- AMQP probe channel fix (4 sites, same pattern each)
- `is_excluded_url_path`: remove `normalize_prefix` from hot path
- `chunk_text`: add early-exit fast path before collecting `Vec<usize>`
- `pending_points` flush threshold: lower default from 2048 → 256
- `try_auto_switch`: accept `Vec<String>` by value
- SSE drain → read cursor pattern

### Medium refactors (require passing new state through call chain)
- `make_pool()` → shared pool at dispatch layer
- Redis connection shared per worker (3 workers)
- `unbounded_channel` → `channel(1)` with `try_send` for progress
- `build_transform_config()` passed to sitemap backfill
- `Vec<char>` → `char_indices()` iterator in snippet utils
- `run_dedupe_native`: streaming dedup without full accumulation

### Larger architectural changes
- `enqueue_job` connection batching
- `run_domains_native` detailed mode guard/rewrite
- Broadcast channel size tied to `max_pages`
- Panic → zombie job fix (wrap processing in `tokio::spawn`)

---

## What Is Working Well

- **`qdrant_scroll_pages`** streaming design is correct — O(page_size) memory per scroll.
- **`HTTP_CLIENT: LazyLock<reqwest::Client>`** in `vector/ops.rs` is the right pattern; the same pattern needs to be applied in the CLI command layer.
- **Batch fetch concurrency** is configurable and well-structured; just needs to be wired into `fetch_batch_results`.
- **AMQP ack/nack** handling in crawl worker is correct — nacks on DB failure prevent message loss.
- **`FOR UPDATE SKIP LOCKED`** in `claim_next_pending` is the right concurrent claim pattern.
- **`ensure_collection()`** idempotent PUT before every upsert is correct.
- **TEI 413 auto-split** in `tei_embed()` correctly halves batch on payload-too-large.
- **Watchdog sweep** every 30s per worker loop catches stale jobs without a separate process.
- **Non-root workers** (s6-setuidgid) and localhost-only port binding are correctly implemented.
