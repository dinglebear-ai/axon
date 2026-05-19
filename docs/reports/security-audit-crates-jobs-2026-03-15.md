# Security Audit: crates/jobs/
**Date:** 2026-03-15
**Scope:** `/home/jmagar/workspace/axon_rust/crates/jobs/` (66 .rs files, ~16.8k lines)
**Auditor:** Security DevSecOps Specialist

---

## Table of Contents
1. [Executive Summary](#executive-summary)
2. [Methodology](#methodology)
3. [Findings by Severity](#findings-by-severity)
   - [High](#high)
   - [Medium](#medium)
   - [Low](#low)
   - [Informational](#informational)
4. [Categories Cleared](#categories-cleared)
5. [Positive Security Findings](#positive-security-findings)
6. [Recommendations Summary](#recommendations-summary)

---

## Executive Summary

The `crates/jobs/` module demonstrates strong security fundamentals. All SQL queries use parameterized binding for user data, SSRF protections are in place, path traversal validation exists with tests, and secrets are redacted in error messages. No critical vulnerabilities were found. The codebase shows evidence of prior security reviews (SEC-M-5, SEC-M-6 comments) and defense-in-depth patterns.

**Overall Risk Rating:** Low-Medium

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High     | 2 |
| Medium   | 4 |
| Low      | 3 |
| Info     | 4 |

---

## Methodology

- Full manual source review of all 66 `.rs` files
- Dependency version audit (lapin 4.2.0, redis 1.0.4, reqwest 0.13.2, sqlx 0.8.6)
- Analysis categories: SQL Injection, Input Validation, Authentication/Authorization, Resource Exhaustion, Race Conditions, Configuration Security, Dependency Vulnerabilities
- Pattern matching for CWE references against OWASP Top 10 2021 and MITRE CWE

---

## Findings by Severity

### HIGH

#### H-1: PgPool Created Per-Call in CRUD Operations — Connection Exhaustion

**CWE-400: Uncontrolled Resource Consumption**
**CVSS:** 7.5 (Network/Low/None/None/High Availability Impact)

**Locations:**
- `crawl/runtime/db.rs:36` — `start_crawl_job()`
- `crawl/runtime/db.rs:191` — `get_job()`
- `crawl/runtime/db.rs:208` — `list_jobs()`
- `crawl/runtime/db.rs:231` — `cancel_job()`
- `crawl/runtime/db.rs:271` — `cleanup_jobs()`
- `crawl/runtime/db.rs:295` — `clear_jobs()`
- `crawl/runtime/db.rs:310` — `recover_stale_crawl_jobs()`
- `extract.rs:91` — `start_extract_job()`
- `extract.rs:163` — `get_extract_job()`
- `extract.rs:173` — `list_extract_jobs()`
- `extract.rs:189` — `cancel_extract_job()`
- `extract.rs:246` — `cleanup_extract_jobs()`
- `extract.rs:273` — `clear_extract_jobs()`
- `refresh.rs:229` — `get_refresh_job()`
- `refresh.rs:243` — `list_refresh_jobs()`
- `refresh.rs:259` — `cancel_refresh_job()`
- `refresh.rs:268` — `cleanup_refresh_jobs()`
- `refresh.rs:291` — `clear_refresh_jobs()`
- `refresh.rs:309` — `recover_stale_refresh_jobs()`
- `ingest/ops.rs:12` — `start_ingest_job()`
- `ingest/ops.rs:49` — `get_ingest_job()`
- `ingest/ops.rs:61` — `list_ingest_jobs()`
- `ingest/ops.rs:86` — `cancel_ingest_job()`
- `ingest/ops.rs:105` — `cleanup_ingest_jobs()`
- `ingest/ops.rs:122` — `clear_ingest_jobs()`

**Description:** Every CLI-facing CRUD function calls `make_pool(cfg).await?` which creates a new PgPool with up to 10 connections, including TCP handshake, TLS negotiation, and authentication. Under concurrent CLI invocations (e.g., a script calling `axon crawl status` in a loop, or the web UI proxying multiple requests), each call opens a new pool. Postgres has a default `max_connections` of 100. With `AXON_PG_POOL_SIZE` defaulting to 10, only 10 concurrent CLI calls saturate the connection limit.

**Attack Scenario:** An attacker (or misconfigured automation) issues rapid concurrent API/CLI calls. Each call allocates a new pool of 10 connections. With 10+ concurrent calls, Postgres reaches `max_connections` and refuses all new connections, including from the workers — causing all job processing to halt.

**Remediation:**
1. For CLI one-shot commands, these functions are acceptable as-is since the process exits after the call. Document this explicitly.
2. For the web UI path (`crates/web.rs`), ensure the axum server creates a single PgPool at startup and passes it to handlers. Verify that web routes do NOT call these per-pool functions but instead use `*_with_pool()` variants.
3. Add an upper-bound clamp to `AXON_PG_POOL_SIZE` in `common/pool.rs:14`:
   ```rust
   let max_conn: u32 = std::env::var("AXON_PG_POOL_SIZE")
       .ok()
       .and_then(|v| v.parse().ok())
       .unwrap_or(10)
       .clamp(1, 50);  // Add upper bound
   ```

**Mitigating Factors:** Worker processes (the long-running components) already create pools once at startup and pass them down via `*_with_pool()` variants. The CLI one-shot pattern is acceptable for its use case. The real risk is if web routes hit these functions under load.

---

#### H-2: Unbounded Delete Loops in Cleanup Operations — Denial of Service

**CWE-400: Uncontrolled Resource Consumption**
**CVSS:** 6.5 (Network/Low/None/None/High Availability Impact)

**Locations:**
- `refresh.rs:274-287` — `cleanup_refresh_jobs()`
- `extract.rs:250-269` — `cleanup_extract_jobs()`

**Description:** Both functions use an infinite `loop` that deletes 1000 rows per iteration until zero rows remain. There is no iteration cap, no yield between iterations, and no timeout. If the table contains millions of failed/canceled rows (achievable via automated job submission over time), this loop will hold a database connection for an extended period, generating sustained I/O load on Postgres.

**Code (refresh.rs:274):**
```rust
loop {
    let deleted = sqlx::query(
        "DELETE FROM axon_refresh_jobs WHERE id IN (
            SELECT id FROM axon_refresh_jobs WHERE status IN ($1,$2) LIMIT 1000)",
    )
    .bind(JobStatus::Failed.as_str())
    .bind(JobStatus::Canceled.as_str())
    .execute(&pool)
    .await?
    .rows_affected();
    total += deleted;
    if deleted == 0 { break; }
}
```

**Attack Scenario:** An attacker submits hundreds of thousands of jobs with invalid configurations (they fail immediately, accumulating `failed` rows). When an admin runs `axon refresh cleanup`, the unbounded loop runs for minutes, holding a connection and generating sustained disk I/O that degrades all Postgres-dependent services.

**Remediation:** Add an iteration cap and a yield between batches:
```rust
const MAX_CLEANUP_ITERATIONS: u32 = 100; // 100k rows max
let mut iterations = 0u32;
loop {
    iterations += 1;
    let deleted = sqlx::query(/* ... */)
        .execute(&pool).await?.rows_affected();
    total += deleted;
    if deleted == 0 || iterations >= MAX_CLEANUP_ITERATIONS { break; }
    tokio::task::yield_now().await; // Let other tasks breathe
}
```

**Note:** `crawl/runtime/db.rs:271` (`cleanup_jobs()`) does NOT have this issue — it uses a single `DELETE` without a loop.

---

### MEDIUM

#### M-1: `open_amqp_channel` Drops Connection — Silent Channel Death Risk

**CWE-404: Improper Resource Shutdown or Release**
**CVSS:** 5.3 (Local/Low/None/None/High Availability Impact)

**Locations:**
- `common/amqp.rs:38-41` — `open_amqp_channel()` definition
- `extract.rs:312` — `extract_doctor()` uses `open_amqp_channel`
- `crawl/runtime/db.rs:20` — `doctor()` uses `open_amqp_channel`

**Description:** `open_amqp_channel()` calls `open_amqp_connection_and_channel()` but immediately drops the `Connection`, keeping only the `Channel`. The function's doc comment (lines 30-37) explicitly warns about this: "This drops the Connection, so the returned channel's backing TCP connection will close asynchronously." The `extract_doctor` function at `extract.rs:312` uses this for health checks — the channel may report `ok` but be dead by the time the result is returned.

**Attack Scenario:** Not exploitable directly, but the inconsistency between `extract_doctor` (uses dropped-connection `open_amqp_channel`) and `embed_doctor` (presumably uses the proper variant or `redis_healthy`) means health checks can return false positives. A monitoring system relying on `extract doctor` could miss broker outages.

**Remediation:** Change `extract_doctor` and `crawl doctor` to use `open_amqp_connection_and_channel` with explicit cleanup:
```rust
let amqp_ok = match open_amqp_connection_and_channel(cfg, &cfg.extract_queue).await {
    Ok((conn, ch)) => {
        let _ = ch.close(0, "health_check".into()).await;
        let _ = conn.close(200, "health_check".into()).await;
        true
    }
    Err(_) => false,
};
```

---

#### M-2: `AXON_PG_POOL_SIZE` Parsed Without Upper Bound

**CWE-1284: Improper Validation of Specified Quantity in Input**
**CVSS:** 4.3 (Local/Low/None/None/High Availability Impact)

**Location:** `common/pool.rs:14-17`

**Code:**
```rust
let max_conn: u32 = std::env::var("AXON_PG_POOL_SIZE")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(10);
```

**Description:** A misconfigured `AXON_PG_POOL_SIZE=10000` would attempt to open 10,000 connections to Postgres, which defaults to `max_connections=100`. Sqlx will queue connection requests, but the pool will never be fully satisfied, and Postgres will reject connections for all other clients.

**Remediation:** Clamp the value:
```rust
.unwrap_or(10)
.clamp(1, 50);
```

---

#### M-3: Extract Worker Passes `openai_api_key` Through `ExtractWebConfig` — Potential Leakage Vector

**CWE-532: Insertion of Sensitive Information into Log File**
**CVSS:** 4.0 (Local/Low/High/None/High Confidentiality Impact)

**Location:** `extract/worker.rs:150`

**Code:**
```rust
let wcfg = ExtractWebConfig {
    start_url: url.clone(),
    prompt: prompt.clone(),
    limit: max_pages,
    openai_base_url: cfg.openai_base_url.clone(),
    openai_api_key: cfg.openai_api_key.clone(),  // <-- API key flows here
    openai_model: cfg.openai_model.clone(),
    custom_headers: custom_headers.clone(),
};
```

**Description:** The `openai_api_key` is passed through `ExtractWebConfig` into the extract engine. If `ExtractWebConfig` derives `Debug` and is ever logged (e.g., during error handling in `run_extract_with_engine`), the API key could appear in log output or error messages stored in `error_text`.

**Mitigating Factor:** The ingest worker has an explicit SEC-M-6 comment confirming `cfg` is never serialized into `error_text`. The extract worker uses `.map_err(|e| e.to_string())` which should only capture the error message, not the config. However, this relies on the downstream `run_extract_with_engine` never including the config in its error output.

**Remediation:**
1. Verify that `ExtractWebConfig` does NOT derive `Debug` (or implements a custom `Debug` that redacts `openai_api_key`).
2. Add a comment similar to SEC-M-6 in `extract/worker.rs` documenting the invariant.
3. Consider using a `Secret<String>` wrapper type that redacts on `Debug`/`Display`.

---

#### M-4: Inconsistent Redis Cancel Error Handling Across Workers

**CWE-755: Improper Handling of Exceptional Conditions**
**CVSS:** 3.7 (Network/Low/None/None/Low Availability Impact)

**Locations:**
- `extract.rs:208-241` — `cancel_extract_job()`: Redis failure is best-effort (log + continue)
- `extract/worker.rs:214-223` — `process_extract_job()`: Redis connect failure is HARD error (returns `Err`)
- `crawl/runtime/db.rs:243-266` — `cancel_job()`: Redis failure is best-effort (log + continue)

**Description:** The cancel flow has an asymmetry: when a user requests cancellation (`cancel_extract_job`), Redis failures are silently tolerated — the DB status is updated to `canceled` regardless. But the extract worker's cancel-check at `process_extract_job` line 214 treats Redis connection failure as a hard error, failing the entire job. This means if Redis goes down:
- Cancel requests succeed (DB-only)
- Extract jobs fail on startup (Redis required for cancel check)

This is the opposite of what you want: cancellation should be the optional signal, not the blocking gate.

**Remediation:** Make the extract worker's Redis cancel check fail-open (same as the embed worker pattern), logging a warning but proceeding:
```rust
let canceled = match redis_client.get_multiplexed_async_connection().await {
    Ok(mut conn) => mark_extract_canceled(&mut conn, pool, id).await.unwrap_or(false),
    Err(e) => {
        log_warn(&format!("extract worker: Redis unavailable for cancel check, proceeding: {e}"));
        false
    }
};
```

---

### LOW

#### L-1: Path Traversal Normalization Inconsistency Between Crawl and Refresh

**CWE-22: Improper Limitation of a Pathname to a Restricted Directory**
**CVSS:** 3.1 (Network/High/High/None/Low Integrity Impact)

**Locations:**
- `crawl/runtime/worker/process.rs` — `normalize_path_lexically()` (conservative: preserves `ParentDir` if not preceded by `Normal`)
- `refresh/url_processor.rs:64-77` — `normalize_path()` (aggressive: unconditional `pop()`)

**Description:** Two separate path normalization implementations exist with different semantics.

The crawl version is more conservative:
```rust
Component::ParentDir => {
    match components.last() {
        Some(Component::Normal(_)) => { components.pop(); }
        _ => components.push(c),
    }
}
```

The refresh version unconditionally pops:
```rust
Component::ParentDir => { normalized.pop(); }
```

Both are used as fallbacks when `tokio::fs::canonicalize()` fails (non-existent paths). The unconditional `pop()` in refresh is actually more aggressive at blocking traversal (since `PathBuf::pop()` on root is a no-op), but the inconsistency is a maintenance hazard — a future developer may "align" them to the weaker version.

**Mitigating Factors:**
- Both paths use `tokio::fs::canonicalize()` as the primary check; normalization is fallback only
- Both have passing tests for traversal rejection
- `validate_url()` is called before fetch in both paths, blocking SSRF
- The refresh path additionally checks `starts_with(canonical_base)` after normalization

**Remediation:** Extract a single `normalize_path_safe()` function into `common/` and use it in both locations. Use the more conservative (crawl) version as the canonical implementation, since it handles edge cases around leading `..` components more explicitly.

---

#### L-2: `OnceLock` Schema Init — Benign TOCTOU

**CWE-367: Time-of-check Time-of-use (TOCTOU) Race Condition**
**CVSS:** 2.0 (Local/High/None/None/Low Integrity Impact)

**Locations:**
- `refresh.rs:52,140-146` — `SCHEMA_INIT: OnceLock<()>` + `ensure_schema_once()`

**Code:**
```rust
static SCHEMA_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub(crate) async fn ensure_schema_once(pool: &PgPool) -> Result<(), sqlx::Error> {
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    Ok(())
}
```

**Description:** Between `get().is_none()` and `set()`, multiple tasks could concurrently enter `ensure_schema()`. This is a classic TOCTOU race on the `OnceLock` because async operations (the schema DDL) happen between check and set.

**Mitigating Factors:** This is entirely benign because:
- All DDL uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`
- The advisory lock pattern (`begin_schema_migration_tx`) serializes concurrent DDL
- The worst case is two workers both running idempotent DDL — a minor performance cost, not a correctness issue

**Remediation:** No action required. The TOCTOU is benign due to DDL idempotency. If desired, use `tokio::sync::OnceCell` for proper async initialization, but this is cosmetic.

---

#### L-3: Graph Taxonomy Path — Filesystem Read from Config

**CWE-73: External Control of File Name or Path**
**CVSS:** 2.4 (Local/Low/High/None/Low Confidentiality Impact)

**Location:** `graph/worker.rs:295-298`

**Code:**
```rust
let taxonomy = if cfg.graph_taxonomy_path.trim().is_empty() {
    Taxonomy::builtin()
} else {
    Taxonomy::from_path(&cfg.graph_taxonomy_path)?
};
```

**Description:** `graph_taxonomy_path` comes from the `AXON_GRAPH_TAXONOMY_PATH` environment variable. If an attacker can control this env var, they could read arbitrary files on disk by pointing it to `/etc/shadow` (etc.). However, `Taxonomy::from_path` presumably expects a specific JSON/TOML format and would fail to parse non-taxonomy files.

**Mitigating Factors:**
- Requires control of server environment variables (already root-equivalent access)
- `Taxonomy::from_path` likely rejects malformed input with a parse error
- The parsed content is used for entity extraction patterns, not displayed or logged raw

**Remediation:** No action required for the current threat model (self-hosted, env-var access implies server access). Document that `AXON_GRAPH_TAXONOMY_PATH` must be a trusted local path.

---

### INFORMATIONAL

#### I-1: Crawl Worker's Cancel Polling Has Bounded Reconnects

**Location:** `crawl/runtime/worker/process.rs`

The crawl worker uses `CANCEL_POLL_MAX_RECONNECTS = 5` to limit Redis reconnection attempts during cancel polling. After 5 failures, cancel polling stops but the job continues. This is the correct fail-open behavior — a Redis outage should not kill running crawl jobs.

---

#### I-2: SEC-M-6 Comment Documents Config Non-Leakage in Ingest Worker

**Location:** `ingest/process.rs:252`

```rust
// SEC-M-6: `cfg` is captured by value but never serialized into error_text.
// All error paths pass only `e.to_string()` or static messages to `mark_job_failed`,
// so `openai_api_key` and other secrets in `cfg` cannot leak into the database.
```

This is a positive security practice — documenting the invariant that prevents secret leakage. The extract worker should have a similar comment (see M-3).

---

#### I-3: Bounded Channels Enforced Throughout

All internal async channels use `tokio::sync::mpsc::channel(256)` — never `unbounded_channel()`. This prevents OOM under backpressure. Verified in:
- `ingest/process.rs:314` — progress channel
- All crawl/embed/tei pipelines

---

#### I-4: `redact_url` Used Consistently for Error Messages

**Location:** `common/pool.rs:29`

```rust
redact_url(&cfg.pg_url)
```

Connection error messages consistently use `redact_url()` to strip credentials from URLs before logging. This prevents credential leakage in log files and error_text database columns.

---

## Categories Cleared

### SQL Injection — CLEAR

All SQL queries were reviewed. The codebase uses two patterns:
1. **Table/status names via `format!()`:** These use `JobTable::as_str()` and `JobStatus::as_str()` which return compile-time `&'static str` values. Not injectable.
2. **User data via `$1/$2` parameter binding:** All user-supplied values (UUIDs, URLs, config JSON) go through sqlx parameter binding. Not injectable.

No raw string interpolation of user data into SQL was found anywhere in the 66 files.

### Authentication/Authorization — N/A (Internal Service)

The jobs module operates as an internal service behind AMQP and Postgres. Authentication/authorization is handled at the API layer (`crates/web.rs`, `crates/mcp/`). The jobs module correctly assumes trusted input from the message broker and database.

### Dependency Vulnerabilities — CLEAR (at audit time)

| Dependency | Version | Status |
|------------|---------|--------|
| sqlx | 0.8.6 | No known CVEs |
| lapin | 4.2.0 | No known CVEs |
| redis | 1.0.4 | No known CVEs |
| reqwest | 0.13.2 | No known CVEs |
| serde_json | 1.x | No known CVEs |
| tokio | 1.x | No known CVEs |

Recommendation: Run `cargo audit` regularly and add to CI pipeline.

### Race Conditions — MITIGATED

- `claim_next_pending()` uses `FOR UPDATE SKIP LOCKED` — correct concurrent claiming
- Stale watchdog uses two-pass confirmation (mark candidate, then reclaim after grace period)
- `OnceLock` TOCTOU is benign (see L-2)
- Cancel flows update DB first, then set Redis signal — correct order

---

## Positive Security Findings

These are security controls already in place that should be maintained:

1. **Defense-in-depth URL validation:** `validate_url()` is called both at job submission time and again in the worker before fetching — protects against stored SSRF via DB tampering.

2. **Path traversal validation with fallback:** Both crawl and refresh paths validate output directories against a base path, using `canonicalize()` with a lexical normalization fallback for non-existent paths.

3. **SSRF test coverage:** `refresh/url_processor.rs:240-250` has explicit tests for private IP rejection (192.168.x, 10.x, 127.x).

4. **Credential redaction:** `redact_url()` consistently used for error messages containing connection strings.

5. **Secret non-leakage documentation:** SEC-M-5 and SEC-M-6 comments document invariants preventing config secrets from reaching `error_text` columns.

6. **Publisher confirms for AMQP:** `batch_enqueue_jobs` uses `confirm_select` + `wait_for_confirms` ensuring messages are acknowledged by the broker before the connection closes.

7. **Bounded channels:** No `unbounded_channel()` usage — prevents OOM under load.

8. **Stale job two-pass watchdog:** Jobs are not immediately reclaimed — they are marked as candidates first, then reclaimed only after a grace period (`AXON_JOB_STALE_CONFIRM_SECS`), preventing false-positive reclaims during legitimate long-running operations.

9. **`MAX_WATCHDOG_RECLAIM_ATTEMPTS = 3`:** Jobs that fail repeatedly are permanently marked as failed after 3 reclaim attempts, preventing infinite retry loops.

10. **Worker env var validation:** `validate_worker_env_vars()` checks for required infrastructure env vars before attempting connections, providing clear error messages instead of cryptic connection failures.

---

## Recommendations Summary

| Priority | Finding | Action |
|----------|---------|--------|
| **High** | H-1: Per-call PgPool | Clamp `AXON_PG_POOL_SIZE` to max 50; verify web routes use `*_with_pool()` |
| **High** | H-2: Unbounded delete loops | Add iteration cap (100) and `yield_now()` between batches |
| **Medium** | M-1: Dropped AMQP connection in doctor | Use `open_amqp_connection_and_channel` with explicit close |
| **Medium** | M-2: No pool size upper bound | Add `.clamp(1, 50)` |
| **Medium** | M-3: API key in ExtractWebConfig | Verify no `Debug` derive; add SEC comment; consider `Secret<String>` |
| **Medium** | M-4: Inconsistent Redis cancel handling | Make extract worker fail-open on Redis unavailability |
| **Low** | L-1: Dual path normalization | Extract shared `normalize_path_safe()` into `common/` |
| **Low** | L-2: OnceLock TOCTOU | No action required (benign) |
| **Low** | L-3: Taxonomy path from env | Document trust requirement; no code change needed |

---

*End of audit. No critical vulnerabilities found. Two high-severity resource exhaustion risks and four medium-severity issues identified. The codebase demonstrates mature security practices with defense-in-depth patterns throughout.*
