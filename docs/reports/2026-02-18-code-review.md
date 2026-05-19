# axon_cli — Comprehensive Code Review
**Date:** 2026-02-18
**Scope:** Full codebase — 33 source files, 5 independent domains
**Method:** 5 parallel `superpowers:code-reviewer` agents

---

## Table of Contents
1. [Executive Summary](#executive-summary)
2. [Critical Issues](#critical-issues)
3. [Security Issues](#security-issues)
4. [Correctness Issues](#correctness-issues)
5. [Quality Issues](#quality-issues)
6. [Consolidated Priority Matrix](#consolidated-priority-matrix)
7. [Domain Reports](#domain-reports)

---

## Executive Summary

The codebase is architecturally coherent and demonstrates genuine engineering care in several areas: the `FOR UPDATE SKIP LOCKED` job queue pattern, the AMQP→Postgres fallback resilience design, the broadcast channel capacity sizing, and the correct `crawl_raw()` / `/chat/completions` URL patterns. However, **5 critical data-corruption or data-loss bugs exist that must be fixed before any production data is written**, and the job infrastructure has a pervasive duplication problem that has already caused security/reliability inconsistencies across the four worker files.

**Do not run this against a real Qdrant collection** until C-VEC-1 (vector ordering bug) is fixed — it will silently corrupt every indexed document.

---

## Critical Issues

> **These must be fixed before the system is used to write any real data.**

### C1 — Vector embedding order is wrong; every indexed document is silently corrupted
**File:** `crates/vector/ops.rs:34–55`
**Domain:** Vector / LLM Extraction

`tei_embed()` uses a LIFO stack for adaptive 413-splitting. Because `Vec::collect()` + `stack.pop()` processes batches in reverse, the `vectors` accumulator is populated in **reverse chunk order** relative to the input. At line 317 of `embed_path_native()`:
```rust
for (idx, (chunk, vecv)) in chunks.into_iter().zip(vectors.into_iter()).enumerate() {
```
Each vector is zipped to the wrong chunk. Wrong embeddings are stored for every chunk. Semantic search returns incorrect content — silently, with no error.

**Fix:** Either iterate chunks sequentially (ordered batching, not LIFO), or tag each batch with its original index and re-sort `vectors` after all batches complete.

---

### C2 — ensure_collection() ignores dimension mismatch on existing Qdrant collection
**File:** `crates/vector/ops.rs:58–66`
**Domain:** Vector / LLM Extraction

```rust
let _ = client.put(url).json(&create).send().await?;
```
The response body is entirely ignored. If the collection already exists with a different vector dimension, Qdrant returns HTTP 200 with `"result": false`. Subsequent upserts with the wrong dimension silently fail or corrupt the collection.

**Fix:** Deserialize the response and check `result["result"] == true`. If false, GET the collection info, compare dimensions, and return a descriptive error on mismatch.

---

### C3 — Broadcast channel lag silently drops pages; CrawlSummary counters undercount
**File:** `crates/crawl/engine.rs:129, 308–310`
**Domain:** Crawl Engine

```rust
Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
```
Pages dropped by the ring buffer are **gone permanently**. `continue` does not retry them. The `Lagged` variant carries the drop count — it is discarded. As a result, `markdown_files` and `thin_pages` undercount, causing `should_fallback_to_chrome()` to make incorrect auto-switch decisions based on corrupt statistics.

**Fix:** Log the lag count at minimum: `log_warn(&format!("broadcast channel lagged, dropped {} pages", n))`. For zero data loss, increase capacity or lower concurrency.

---

### C4 — Output directory is deleted before crawl completes; partial failure leaves corrupt state
**File:** `crates/crawl/engine.rs:288–291`
**Domain:** Crawl Engine

```rust
if output_dir.exists() {
    fs::remove_dir_all(output_dir)?;
}
fs::create_dir_all(output_dir.join("markdown"))?;
```
The directory is destroyed before any crawl data is written. Any failure between line 291 and crawl completion leaves a partially-written directory. The next run deletes and restarts, but consumers of the output directory see corrupt state during the failure window.

**Fix:** Write to a temp directory (`output_dir.with_extension("tmp")`); atomically rename to the final path on success; delete temp dir on failure.

---

### C5 — manifest.jsonl BufWriter dropped without flush on error paths
**File:** `crates/crawl/engine.rs:342–349`
**Domain:** Crawl Engine

The `?` operator inside `tokio::spawn` skips `manifest.flush()` on any write error. Rust's `BufWriter::drop` silently discards flush errors (documented stdlib behavior). Result: `manifest.jsonl` is truncated mid-record on any mid-crawl I/O failure.

**Fix:** Restructure so `manifest.flush()` runs on all exit paths, or return the manifest from the closure and flush it in the outer function after `join.await`.

---

### C6 — No response size cap; unbounded memory growth on large pages
**File:** `crates/core/http.rs:10–18`
**Domain:** Core Infrastructure

```rust
.text().await?   // reads entire response body into heap String with no limit
```
`.text().await?` buffers the full response. A 500 MB HTML dump or adversarial URL exhausts available memory. This function is called from `scrape.rs`, `batch.rs`, `search.rs`, `batch_jobs.rs`, and `vector/ops.rs`. Combined with `Max` profile's 1024 concurrent tasks, a worst-case scenario is catastrophic.

**Fix:** Accumulate with a byte-count guard; reject responses over a configurable limit (e.g., 50 MB). Use `bytes_stream()` for streaming accumulation.

---

### C7 — Non-durable queues and non-persistent messages; all queued jobs lost on RabbitMQ restart
**File:** `crates/jobs/crawl_jobs.rs:143–160`, same in all 4 job files
**Domain:** Async Job Workers

```rust
ch.queue_declare(name, QueueDeclareOptions {
    durable: false, ..Default::default()
}, FieldTable::default()).await?;
```
```rust
ch.basic_publish(..., BasicProperties::default()).await?;
// BasicProperties::default() has delivery_mode=1 (transient, not persistent)
```
Every queued job vanishes on RabbitMQ restart. This is the single highest-impact reliability issue in the codebase.

**Fix:**
```rust
QueueDeclareOptions { durable: true, ..Default::default() }
BasicProperties::default().with_delivery_mode(2)  // persistent
```
**Note:** Must use a new queue name or delete/recreate queues on deploy — RabbitMQ rejects option changes on existing queues.

---

### C8 — passthrough.rs references non-existent bridge module and Config field
**File:** `crates/cli/commands/passthrough.rs`
**Domain:** CLI Commands

```rust
use crate::axon_cli::bridge::{quote_shell, run_axon_command};  // module does not exist
// ...
cfg.raw_args  // field does not exist on Config
```
This file **will not compile** if it is ever included in a module declaration. It is currently not exported from `commands/mod.rs` — it is dead code waiting to cause a compile error on the next developer who wires it up.

**Fix:** Either delete the file or implement `crate::axon_cli::bridge` and add the `raw_args: Vec<String>` field to `Config`.

---

### C9 — mark_*_job_failed is fire-and-forget; silent failure leaves jobs stuck in 'running' forever
**File:** `crates/jobs/crawl_jobs.rs:461–469`, same in all 4 job files
**Domain:** Async Job Workers

```rust
async fn mark_crawl_job_failed(pool: &PgPool, id: Uuid, error_text: &str) {
    let _ = sqlx::query(...).execute(pool).await;  // result silently discarded
}
```
If the UPDATE fails, the job stays `status='running'` indefinitely. `cleanup_jobs()` does not touch `running` jobs. Ghost records accumulate.

**Fix:** Log the error. Add stale `running` cleanup: `OR (status='running' AND updated_at < NOW() - INTERVAL '2 hours')`.

---

### C10 — cleanup_jobs() leaves orphaned crawl output directories on disk
**File:** `crates/jobs/crawl_jobs.rs:282–292`, `batch_jobs.rs:174–183`
**Domain:** Async Job Workers

`DELETE FROM axon_crawl_jobs` removes the DB row but not the corresponding `{output_dir}/jobs/{uuid}/` directory. Repeated job cycles silently accumulate gigabytes of orphaned crawl output in `.cache/axon-rust/output`.

**Fix:** Collect `output_dir` from each job's `config_json` before deleting the row; remove corresponding directories. Also add stale `running` jobs to the DELETE clause.

---

### C11 — Silent batch URL fetch failure swallowing
**File:** `crates/cli/commands/batch.rs` (URL fetch loop)
**Domain:** CLI Commands

The URL fetch loop silently swallows errors — failed fetches increment no counter and emit no warning, meaning a batch run that failed on 80% of URLs reports apparent success.

**Fix:** Track and report failed URLs count; log each fetch failure with URL and error.

---

## Security Issues

### S1 — openai_api_key leaks via Config's Debug derive
**Files:** `crates/core/config.rs:73`, `crates/jobs/crawl_jobs.rs` (all 4 job files)
**Severity:** High

`Config` derives `Debug`. Any `{:?}` in a panic message, tracing span, or test output prints `openai_api_key` in plaintext. Same applies to `pg_url`, `amqp_url`, and `redis_url` which may contain embedded credentials.

**Fix:** Implement `Debug` manually for `Config` with redacted sensitive fields, or wrap them in a `Secret<String>` newtype.

---

### S2 — API key accepted as a CLI flag (visible in ps, shell history, audit logs)
**File:** `crates/core/config.rs:346–351`
**Severity:** High

```rust
#[arg(global = true, long)]
openai_api_key: Option<String>,
```
CLI arguments are visible to all users via `ps aux`, saved in shell history, and captured by audit logging. This is worse than an environment variable.

**Fix:** Remove the `--openai-api-key` CLI flag. Use `OPENAI_API_KEY` environment variable exclusively.

---

### S3 — Raw HTML with PII/tokens sent to LLM provider
**File:** `crates/extract/remote_extract.rs:35–51`
**Severity:** Medium–High (context-dependent)

```rust
let trimmed_html: String = html.chars().take(20_000).collect();
// sent directly to external LLM API
```
Raw HTML can contain CSRF tokens, `<meta>` PII, structured data (schema.org names/emails), hidden fields, and JavaScript variables. These are forwarded to whatever endpoint `OPENAI_BASE_URL` points to.

**Fix:** Convert to markdown before sending — `to_markdown()` already exists in `core/content.rs`. Strip `<script>`, `<style>`, `<meta>`, HTML comments, and all non-semantic attributes before transmission.

---

### S4 — Service credentials exposed in doctor/status output
**File:** `crates/cli/commands/doctor.rs`, `crates/jobs/crawl_jobs.rs` error messages
**Severity:** Medium

The doctor command assembles a diagnostic blob including full connection strings (`pg_url`, `amqp_url`, `redis_url`) which contain embedded credentials. Same issue in connection timeout error messages: `format!("postgres connect timeout: {} ...", cfg.pg_url)`.

**Fix:** Redact credentials from all user-visible output. Parse URLs and replace the userinfo component with `user:***@host:port` before displaying.

---

### S5 — Hardcoded binary name "cortex" in crawl status output
**File:** `crates/cli/commands/crawl.rs`
**Domain:** CLI Commands

Output prints `"Check status: cortex crawl status {job_id}"`. If the binary is named differently (as it will be when spun off as a standalone repo), this instruction is wrong and confusing.

**Fix:** Use `env::args().next()` or a build-time constant for the binary name.

---

### S6 — Path traversal via output_dir in stored config_json
**File:** `crates/jobs/crawl_jobs.rs:359–361`
**Severity:** Low for single-user CLI, High if exposed as API

```rust
job_cfg.output_dir = PathBuf::from(parsed.output_dir)
    .join("jobs").join(id.to_string());
```
If `config_json` in Postgres contains a crafted `output_dir: "../../etc"`, crawler output writes to an attacker-controlled path.

**Fix:** Validate the resolved output path is a subdirectory of an allowed base using `Path::starts_with()` on the canonicalized path.

---

## Correctness Issues

### R1 — search_limit hard-coded; cfg.search_limit is ignored in query and ask commands
**File:** `crates/vector/ops.rs:367, 562`

```rust
let hits = qdrant_search(cfg, &vector, 10).await?;  // run_query_native
let hits = qdrant_search(cfg, &vecq, 8).await?;     // run_ask_native
```
Users who pass `--limit 25` always get 10 or 8 results. The `qdrant_search` function signature accepts a limit parameter — it is just not being passed.

**Fix:** `qdrant_search(cfg, &vector, cfg.search_limit)` and `qdrant_search(cfg, &vecq, cfg.search_limit.min(8))`.

---

### R2 — qdrant_upsert() sends all points in one request; 413 at scale
**File:** `crates/vector/ops.rs:68–85`

All accumulated vectors sent to Qdrant in a single PUT. At 500 pages × 10 chunks × 1536 dimensions × 4 bytes ≈ 31 MB — well above typical Qdrant HTTP body limits.

**Fix:** Batch upserts in chunks of ~100 points per request.

---

### R3 — qdrant_scroll_all() loads entire collection into memory
**File:** `crates/vector/ops.rs:87–132`

Both `run_sources_native()` and `run_domains_native()` call `qdrant_scroll_all()` which accumulates all collection points into a single `Vec`. At 1M vectors with ~500 byte payloads this is ~500 MB heap.

**Fix:** Stream-aggregate: maintain the grouping `BTreeMap` inside the scroll loop rather than collecting all points first.

---

### R4 — run_remote_extract() channel capacity of 16 causes page loss under LLM latency
**File:** `crates/extract/remote_extract.rs:97`

```rust
let mut rx = website.subscribe(16).ok_or("subscribe failed")?;
```
With LLM round-trips at 2–5 seconds per page, the crawler fills the 16-item buffer. Spider.rs's bounded channel will drop sends, silently losing pages with no indication in output counts.

**Fix:** Increase capacity to at least `cfg.max_pages as usize` or a configurable value (256+). Decouple page receipt from LLM calls with an intermediate queue.

---

### R5 — unsubscribe() called before collect.await; races channel drain
**File:** `crates/extract/remote_extract.rs:147–150`

```rust
website.unsubscribe();           // drops sender — channel closed
let (results, _) = collect.await?;  // collector may not have drained all pages yet
```
Pages remaining in the buffer when `unsubscribe()` is called may not be processed by the collector.

**Fix:** Call `unsubscribe()` after `collect.await` completes.

---

### R6 — Polling worker loop exits on Postgres error; no backoff on transient failures
**File:** `crates/jobs/crawl_jobs.rs:471–484`, same in all 4 job files

```rust
if let Some(job_id) = claim_next_pending_job(pool).await? {
```
Any Postgres error (transient connection drop, timeout) exits the polling loop via `?`. The worker dies. A brief DB hiccup kills all polling workers with no backoff or retry.

**Fix:** Distinguish "no jobs" (sleep 800ms) from "database error" (log warning, sleep with exponential backoff, do not exit).

---

### R7 — Consumer error in batch/extract/embed jobs causes CPU-spinning silent loop
**File:** `crates/jobs/batch_jobs.rs:327–330`, `extract_jobs.rs:367–370`, `embed_jobs.rs:311–315`

```rust
Err(_) => continue,  // no ack — message redelivered; no sleep
```
On persistent AMQP channel failure, `consumer.next()` returns `Err` on every iteration with no sleep, creating a CPU-busy loop. Only `crawl_jobs.rs` logs the error; the other three silently spin.

**Fix:** Log the error (match `crawl_jobs.rs` behavior). On channel-level errors, break and reconnect with backoff.

---

### R8 — fetch_retries == 0 sentinel overrides legitimate "zero retries" user intent
**File:** `crates/core/config.rs:612–614`

```rust
if cfg.fetch_retries == 0 {
    cfg.fetch_retries = retries_default;
}
```
A user who explicitly sets `--fetch-retries 0` for fail-fast behavior silently receives the profile default retry count instead.

**Fix:** Use `Option<usize>` for `fetch_retries` (`None` = use profile default, `Some(0)` = zero retries).

---

### R9 — Hardcoded 20s timeout ignores cfg.request_timeout_ms
**File:** `crates/core/http.rs:6`, all callers except `crawl/engine.rs`

`build_client(20)` is hard-coded in `scrape.rs`, `batch.rs`, `search.rs`, `batch_jobs.rs`, and `vector/ops.rs`. The config's `request_timeout_ms` is carefully computed per performance profile and then silently discarded.

**Fix:** Pass `cfg.request_timeout_ms / 1000` to `build_client()` at all call sites.

---

### R10 — read_inputs() does not recurse into subdirectories
**File:** `crates/vector/ops.rs:226–237`

`fs::read_dir()` is non-recursive. Documentation directories with subdirectories (`docs/api/`, `docs/guides/`) silently have their files skipped. The user sees fewer chunks than expected with no error.

**Fix:** Use `walkdir` crate for recursive traversal, or document the limitation.

---

### R11 — extract_links case-sensitive; misses HREF= and single-quoted attributes
**File:** `crates/core/content.rs:74–95`

```rust
while let Some(rel) = html[pos..].find("href=\"") {
```
Misses `HREF=`, `Href=`, `href='...'`. For a RAG pipeline, missing links means missing pages means incomplete index coverage.

**Fix:** Case-fold the search or use a case-insensitive match.

---

### R12 — extract_meta_description fails silently on reversed HTML attribute order
**File:** `crates/core/content.rs:63–72`

`<meta content="..." name="description">` (valid HTML; reversed attribute order) returns `None` even though a description is present.

**Fix:** After finding `<meta`, search for both `name="description"` and `content="..."` within the tag boundaries, regardless of order.

---

### R13 — extract_loc_values does not strip CDATA wrappers; returns invalid URLs
**File:** `crates/core/content.rs:97–114`

```xml
<loc><![CDATA[https://example.com/page]]></loc>
```
The extractor returns `<![CDATA[https://example.com/page]]>` as the URL value — not a valid URL.

**Fix:** Strip `<![CDATA[` / `]]>` wrappers after extraction.

---

### R14 — process_job uses stored output_dir that may be wrong on worker host
**File:** `crates/jobs/crawl_jobs.rs:359–361`

The `output_dir` stored in `config_json` is path from the submitting machine. On a different worker host with a different filesystem layout, this path is invalid.

**Fix:** Use `cfg.output_dir` (current worker's configured base) and append `jobs/{uuid}`, ignoring `parsed.output_dir`.

---

### R15 — AMQP connection timeout missing from batch/extract/embed workers
**File:** `crates/jobs/batch_jobs.rs:82`, `extract_jobs.rs:77`, `embed_jobs.rs:76`

`crawl_jobs.rs` correctly wraps `Connection::connect` in a 5-second timeout. The other three files connect without any timeout, hanging indefinitely if RabbitMQ is slow to respond.

**Fix:** Apply the same `tokio::time::timeout(Duration::from_secs(5), Connection::connect(...))` pattern from `crawl_jobs.rs`.

---

## Quality Issues

### Q1 — ~320 lines of copy-paste infrastructure across 4 job files
**Files:** All 4 job files
**Severity:** High (root cause of Q2, R7, R15, and others)

`pool()`, `open_channel()`, `ensure_schema()`, `enqueue()`, `claim_next_pending_job()`, `claim_pending_by_id()`, `mark_*_failed()`, and the worker loop scaffolding are independently copy-pasted in each file. A fix in one file (e.g., the AMQP connect timeout in `crawl_jobs.rs`) is not propagated to the others — which is exactly how R15 occurred.

**Fix:** Extract a `JobStore<T>` or `JobInfra` struct to `crates/jobs/infra.rs` with table-name-parameterized implementations. Individual job files contain only job-specific `process_*` logic.

---

### Q2 — DefaultHasher produces different filenames across Rust versions and process runs
**Files:** `crates/core/content.rs:51–53`, `crates/crawl/engine.rs`

`DefaultHasher` is seeded differently per-process since Rust 1.36. The same URL produces different hash suffixes on different runs. Incremental re-crawl logic cannot rely on filenames to detect previously-processed URLs.

**Fix:** Use a deterministic hash (FxHasher, CRC32, or last 8 hex chars of SHA-256).

---

### Q3 — 720+ lines of duplicated job subcommand handling across 4 CLI command files
**Files:** `crawl.rs`, `extract.rs`, `batch.rs`, `embed.rs` CLI command files
**Domain:** CLI Commands

The status/cancel/list/cleanup/clear/worker subcommand pattern is copy-pasted across all 4 files with only table names and function names differing.

**Fix:** Extract a `run_job_subcommands(cfg, job_type, ...)` generic handler.

---

### Q4 — collect_items() empty-object check accepts garbage LLM envelope responses
**File:** `crates/extract/remote_extract.rs:19`

```rust
} else if !value.is_null() && value != &serde_json::Value::Object(serde_json::Map::new()) {
```
`{"metadata": {}, "results": null}` passes this check and returns the entire metadata envelope as a result item.

**Fix:** Check that at least one expected key (`"results"`, `"items"`, `"data"`) is present and non-null.

---

### Q5 — OPENAI_BASE_URL guard missing; /chat/completions suffix causes 404
**File:** `crates/vector/ops.rs:578–581`, `crates/extract/remote_extract.rs`

If a user sets `OPENAI_BASE_URL=http://host/v1/chat/completions`, the constructed URL becomes `.../chat/completions/chat/completions` — a 404 with no clear error.

**Fix:** At the entry to `run_ask_native()` and `run_remote_extract()`, check if `openai_base_url` ends with `/chat/completions` and return: `"OPENAI_BASE_URL must not include /chat/completions — it is appended automatically"`.

---

### Q6 — Spinner has no Drop impl; leaves ambiguous terminal state on error
**File:** `crates/core/ui.rs:8–27`

If any `?` between `Spinner::new()` and `.finish()` returns an error, the spinner is dropped without a completion message. Users see a blank line with no indication of success or failure.

**Fix:**
```rust
impl Drop for Spinner {
    fn drop(&mut self) {
        if !self.bar.is_finished() {
            self.bar.abandon_with_message("interrupted");
        }
    }
}
```

---

### Q7 — ensure_schema() called on every read-only command; DDL lock contention
**File:** `crates/jobs/crawl_jobs.rs:229–241`, same in all 4 job files

Every `get_job()`, `list_jobs()`, `cancel_job()` issues `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS`. With 5 workers running concurrently, this causes measurable catalog lock contention on every status check.

**Fix:** Run schema initialization once at startup using a migration tool or `OnceLock`-guarded flag.

---

### Q8 — cleanup_batch/extract/embed jobs missing stale pending job cleanup
**Files:** `batch_jobs.rs:177–182`, `extract_jobs.rs:175–184`, `embed_jobs.rs:165–174`

`crawl_jobs.rs` correctly adds: `OR (status='pending' AND created_at < NOW() - INTERVAL '1 day')`. The other three files only delete `failed` and `canceled` — stale pending jobs accumulate forever.

**Fix:** Add the stale pending clause to all four cleanup functions.

---

### Q9 — reqwest::Client created fresh on every function call
**File:** `crates/vector/ops.rs` (multiple locations)

A new `reqwest::Client` is created in `tei_embed()`, `qdrant_upsert()`, `qdrant_scroll_all()`, etc. This defeats TCP/TLS connection reuse and allocates a new thread pool per call. For a 10,000-chunk embed job this adds thousands of unnecessary TCP handshakes.

**Fix:** Pass a shared `reqwest::Client` as a parameter, or use a module-level `OnceLock<reqwest::Client>`.

---

### Q10 — process_batch_job silently swallows embed errors; reports 'completed' incorrectly
**File:** `crates/jobs/batch_jobs.rs:290–293`

```rust
let _ = embed_path_native(&embed_cfg, &out_dir.to_string_lossy()).await;
```
A batch job that finishes scraping but fails embedding reports `status='completed'`. Contrast with `crawl_jobs.rs` where embed errors propagate via `?` and mark the job `failed`.

**Fix:** Propagate the error via `?` (consistent with `crawl_jobs.rs`).

---

### Q11 — "As of: now" placeholder string in scrape.rs
**File:** `crates/cli/commands/scrape.rs`
**Domain:** CLI Commands

Literal `"As of: now"` placeholder in output — clearly unfinished.

**Fix:** Replace with `chrono::Local::now().format(...)` or similar.

---

### Q12 — status.rs issues four sequential DB queries; should be parallel
**File:** `crates/cli/commands/status.rs`
**Domain:** CLI Commands

The status command queries all four job tables sequentially. At 5-second pool timeouts each, worst case is 20 seconds to render status. These four queries are independent.

**Fix:** Use `tokio::join!` to run all four queries concurrently.

---

### Q13 — CORTEX_NO_COLOR inconsistent with NO_COLOR standard
**File:** `crates/core/config.rs:637`

The `console` crate respects the standard `NO_COLOR` environment variable. `config.rs` uses a custom `CORTEX_NO_COLOR` variable for the custom help text. The two color systems diverge: `NO_COLOR=1` disables `console`-based colors in `ui.rs`/`logging.rs` but has no effect on the help text.

**Fix:** Use `NO_COLOR` in `print_top_level_help`, or check both.

---

## Consolidated Priority Matrix

| Priority | ID | File(s) | Issue |
|----------|-----|---------|-------|
| 🔴 CRITICAL | C1 | `vector/ops.rs:34` | Vector order wrong — silent corruption of every indexed document |
| 🔴 CRITICAL | C2 | `vector/ops.rs:58` | ensure_collection ignores dimension mismatch |
| 🔴 CRITICAL | C3 | `crawl/engine.rs:129,308` | Broadcast lag silently drops pages; statistics corrupted |
| 🔴 CRITICAL | C4 | `crawl/engine.rs:288` | Output dir deleted before crawl; partial failure = corrupt state |
| 🔴 CRITICAL | C5 | `crawl/engine.rs:342` | manifest.jsonl truncated on error — BufWriter not flushed |
| 🔴 CRITICAL | C6 | `core/http.rs:10` | No response size cap; unbounded memory growth |
| 🔴 CRITICAL | C7 | All job files | Non-durable queues; all jobs lost on RabbitMQ restart |
| 🔴 CRITICAL | C8 | `cli/commands/passthrough.rs` | Dead code with compile errors; references non-existent module |
| 🔴 CRITICAL | C9 | All job files | mark_*_job_failed fire-and-forget; stuck running jobs forever |
| 🔴 CRITICAL | C10 | `jobs/crawl_jobs.rs:282` | cleanup_jobs leaves orphaned GB of disk output |
| 🔴 CRITICAL | C11 | `cli/commands/batch.rs` | Silent batch URL fetch failure swallowing |
| 🔴 SECURITY | S1 | `core/config.rs:73` | openai_api_key leaks via Config Debug derive |
| 🔴 SECURITY | S2 | `core/config.rs:347` | API key accepted as CLI flag (visible in ps/history) |
| 🟡 SECURITY | S3 | `extract/remote_extract.rs:35` | Raw HTML with PII/tokens sent to external LLM |
| 🟡 SECURITY | S4 | `jobs/`, `cli/doctor.rs` | Service credentials in error messages and doctor output |
| 🟡 SECURITY | S5 | `cli/commands/crawl.rs` | Hardcoded binary name "cortex" in output |
| 🟡 SECURITY | S6 | `jobs/crawl_jobs.rs:359` | Path traversal via output_dir in stored config_json |
| 🟠 CORR | R1 | `vector/ops.rs:367,562` | cfg.search_limit ignored; hard-coded limits |
| 🟠 CORR | R2 | `vector/ops.rs:68` | All points upserted in one request; 413 at scale |
| 🟠 CORR | R3 | `vector/ops.rs:87` | qdrant_scroll_all loads full collection into memory |
| 🟠 CORR | R4 | `extract/remote_extract.rs:97` | Channel capacity 16 drops pages under LLM latency |
| 🟠 CORR | R5 | `extract/remote_extract.rs:147` | unsubscribe before collect.await races channel drain |
| 🟠 CORR | R6 | All job files | Polling loop exits on Postgres error; no backoff |
| 🟠 CORR | R7 | `batch/extract/embed_jobs.rs` | Consumer error causes CPU-spinning silent loop |
| 🟠 CORR | R8 | `core/config.rs:612` | fetch_retries == 0 overrides explicit user intent |
| 🟠 CORR | R9 | `core/http.rs:6`, callers | 20s timeout hardcoded; cfg.request_timeout_ms ignored |
| 🟠 CORR | R10 | `vector/ops.rs:226` | read_inputs non-recursive; silently skips subdirectories |
| 🟠 CORR | R11 | `core/content.rs:74` | extract_links case-sensitive; misses HREF= and single-quoted |
| 🟠 CORR | R12 | `core/content.rs:63` | extract_meta_description fails on reversed attribute order |
| 🟠 CORR | R13 | `core/content.rs:97` | extract_loc_values doesn't strip CDATA; returns invalid URLs |
| 🟠 CORR | R14 | `jobs/crawl_jobs.rs:359` | stored output_dir invalid on different worker host |
| 🟠 CORR | R15 | `batch/extract/embed_jobs.rs:77` | AMQP connection timeout missing (only crawl has it) |
| 🔵 QUALITY | Q1 | All job files | ~320 lines copy-paste infra; root cause of 5+ other bugs |
| 🔵 QUALITY | Q2 | `core/content.rs:51` | DefaultHasher non-deterministic; filenames change across runs |
| 🔵 QUALITY | Q3 | 4 CLI command files | 720 lines duplicated job subcommand handling |
| 🔵 QUALITY | Q4 | `extract/remote_extract.rs:19` | collect_items accepts garbage LLM envelope responses |
| 🔵 QUALITY | Q5 | `vector/ops.rs:578` | No guard against /chat/completions in base URL |
| 🔵 QUALITY | Q6 | `core/ui.rs:8` | Spinner has no Drop; silent failure on error paths |
| 🔵 QUALITY | Q7 | All job files | ensure_schema DDL on every read-only command |
| 🔵 QUALITY | Q8 | `batch/extract/embed_jobs.rs` | Missing stale pending job cleanup clause |
| 🔵 QUALITY | Q9 | `vector/ops.rs` (multiple) | reqwest::Client created per call; defeats connection reuse |
| 🔵 QUALITY | Q10 | `jobs/batch_jobs.rs:290` | embed errors silently swallowed; job reports completed |
| 🔵 QUALITY | Q11 | `cli/commands/scrape.rs` | "As of: now" placeholder in output |
| 🔵 QUALITY | Q12 | `cli/commands/status.rs` | 4 sequential DB queries should be tokio::join! |
| 🔵 QUALITY | Q13 | `core/config.rs:637` | CORTEX_NO_COLOR inconsistent with NO_COLOR standard |

---

## Domain Reports

Detailed findings by domain:

| Domain | Agent | Issues |
|--------|-------|--------|
| CLI Commands (scrape, crawl, map, batch, extract, embed, search, doctor, status, passthrough) | a0e3b43 | C8, C11, S4, S5, Q3, Q11, Q12 |
| Core Infrastructure (config, content, http, health, logging, ui) | a37dab9 | C6, S1, S2, R8, R9, R11, R12, R13, Q2, Q6, Q13 + more |
| Crawl Engine (engine.rs) | aa89d14 | C3, C4, C5, R3 (scroll), R13 (loc/cdata) |
| Vector Ops + LLM Extraction (vector/ops.rs, remote_extract.rs) | a781a55 | C1, C2, S3, R1, R2, R3, R4, R5, R10, Q4, Q5, Q9 |
| Async Job Workers (crawl_jobs, batch_jobs, extract_jobs, embed_jobs) | ac5cf2a | C7, C9, C10, S4, S6, R6, R7, R14, R15, Q1, Q7, Q8, Q10 |

---

*Report generated: 2026-02-18. All findings are from automated parallel code review agents with manual synthesis.*
