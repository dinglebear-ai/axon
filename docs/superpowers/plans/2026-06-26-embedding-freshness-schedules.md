# Embedding Freshness Schedules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CLI-only `--fresh <Nd>` scheduling for embedding-producing Axon operations so `scrape`, `crawl`, `embed`, and `ingest` can keep indexed origins fresh without external cron.

**Architecture:** CLI creates schedules; `axon-services` owns freshness orchestration and scheduler startup; `axon-jobs` owns only SQLite persistence, lease primitives, and run history. Freshness is separate from `axon watch`: watch remains URL change detection, while freshness replays versioned canonical embedding requests through existing service/job entry points with single-flight leases, safe replay snapshots, bounded concurrency, and jittered due times.

**Tech Stack:** Rust 2024, clap, serde JSON, sha2 identity hashes, sqlx SQLite migrations, Tokio service scheduler, existing `ServiceContext`/job runtime, Axon redaction helpers, Qdrant deterministic upsert IDs.

## Global Constraints

- V1 is CLI-only: `--fresh` creation plus `axon fresh list`, `axon fresh run-now <id>`, and `axon fresh history <id>`. REST/MCP/web/palette surfaces are follow-up beads.
- Preserve current `axon watch` semantics: URL change detection remains diff-gated and crawl-triggering.
- `--fresh` accepts whole-day durations in the range `1d` through `366d`; reject zero, sub-day, fractional, uppercase-unit, and unknown-unit values with clap-style errors.
- Freshness schedules must replay the original effective request options, collection, render mode, source-type options, crawl bounds, and other replay-affecting config without persisting secrets.
- Persist only safe replay snapshots: no bearer tokens, cookies, API keys, Reddit/GitHub/OpenAI keys, or secret-bearing headers in SQLite, list/history output, logs, or run errors.
- Revalidate all persisted replay payloads at dispatch time. SQLite is not a trusted boundary.
- Freshness must route through existing services/jobs where job families exist, preserving queue caps, snapshots, validation, and worker notifications.
- Scheduled scrape has no existing job family in v1, so it must run only inside the freshness-owned bounded executor with explicit semaphore, heartbeat, timeout, and run history.
- RSS ingest must have explicit regression coverage for repeat ingest, duplicate feed entries, and URL-normalization variants.
- Existing `axon diff` already exists across CLI, REST, MCP, and service layers; document it but do not reimplement it.
- Full closeout requires clean linter, green tests, execution-proof live smokes, PR linked to a self-contained GitHub issue, CI passing, and all related beads updated or closed.

---

## File Structure

- Create `crates/axon-core/src/config/types/freshness.rs`: `FreshDuration`, `FreshnessCommand`, `FreshnessRequest`.
- Modify `crates/axon-core/src/config/types.rs` and `crates/axon-core/src/config/types/config.rs`: expose and store optional freshness intent.
- Modify `crates/axon-core/src/config/cli.rs`: add CLI-only `--fresh <duration>` to `scrape`, `crawl`, `embed`, and `ingest`; add `fresh` subcommand.
- Modify `crates/axon-core/src/config/parse/build_config/{command_dispatch,config_literal}.rs`: parse and populate `Config::freshness`.
- Create `crates/axon-jobs/src/freshness.rs` and `crates/axon-jobs/src/freshness/rows.rs`: storage, validation, lease, heartbeat, reclaim, history.
- Create `crates/axon-jobs/src/migrations/NNN_freshness.sql`: schedule and run tables with `identity_hash`.
- Create `crates/axon-services/src/freshness.rs`: safe snapshot builder, typed versioned payloads, scheduler loop, bounded dispatch, list/history/run-now service functions.
- Modify `crates/axon-services/src/context.rs`: spawn the freshness scheduler when `ServiceContext::new_with_workers()` is used.
- Create `crates/axon-cli/src/commands/fresh.rs` and wire `CommandKind::Fresh` in `crates/axon-cli/src/lib.rs`.
- Modify `crates/axon-cli/src/commands/{scrape,crawl,embed,ingest}.rs`: create schedules when `cfg.freshness` is present.
- Modify `crates/axon-ingest/src/rss.rs` and `crates/axon-ingest/src/rss_tests.rs`: pin RSS dedupe semantics.
- Update `docs/reference/actions/{scrape,crawl,embed,ingest,diff}.md`, create `docs/reference/actions/fresh.md`, update `CLAUDE.md`, `README.md`, `.env.example`, `config.example.toml`, and `docs/guides/configuration.md`.
- Create `scripts/smoke-freshness.sh`: isolated execution smoke with temp SQLite data dir and cleanup.

### Task 1: Fresh Duration Parsing and CLI Intent

**Files:**
- Create: `crates/axon-core/src/config/types/freshness.rs`
- Modify: `crates/axon-core/src/config/types.rs`
- Modify: `crates/axon-core/src/config/types/config.rs`
- Modify: `crates/axon-core/src/config/cli.rs`
- Modify: `crates/axon-core/src/config/parse/build_config/command_dispatch.rs`
- Modify: `crates/axon-core/src/config/parse/build_config/config_literal.rs`
- Test: `crates/axon-core/src/config/types_tests.rs`
- Test: `crates/axon-core/src/config/parse/env_registry_tests.rs`

**Interfaces:**
- Produces: `FreshDuration`, `FreshnessCommand`, `FreshnessRequest`.
- Consumes: existing `Config`, `CommandKind`, and clap parsing.

- [ ] **Step 1: Write failing parser tests**

Add to `crates/axon-core/src/config/types_tests.rs`:

```rust
use super::freshness::FreshDuration;

#[test]
fn fresh_duration_accepts_whole_days() {
    assert_eq!(FreshDuration::parse("1d").unwrap().days, 1);
    assert_eq!(FreshDuration::parse("7d").unwrap().seconds, 7 * 24 * 60 * 60);
    assert_eq!(FreshDuration::parse("366d").unwrap().days, 366);
}

#[test]
fn fresh_duration_rejects_invalid_values() {
    for raw in ["", "0d", "367d", "1h", "24h", "0.5d", "1D", "day", "7"] {
        let err = FreshDuration::parse(raw).unwrap_err();
        assert!(err.contains("--fresh expects a whole-day duration from 1d to 366d"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-core fresh_duration -- --nocapture`

Expected: FAIL because `FreshDuration` does not exist.

- [ ] **Step 3: Add freshness types**

Create `crates/axon-core/src/config/types/freshness.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshDuration {
    pub days: u32,
    pub seconds: i64,
}

impl FreshDuration {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let Some(days_raw) = raw.strip_suffix('d') else {
            return Err("--fresh expects a whole-day duration from 1d to 366d".to_string());
        };
        if days_raw.is_empty() || days_raw.contains('.') {
            return Err("--fresh expects a whole-day duration from 1d to 366d".to_string());
        }
        let days: u32 = days_raw
            .parse()
            .map_err(|_| "--fresh expects a whole-day duration from 1d to 366d".to_string())?;
        if !(1..=366).contains(&days) {
            return Err("--fresh expects a whole-day duration from 1d to 366d".to_string());
        }
        Ok(Self {
            days,
            seconds: i64::from(days) * 24 * 60 * 60,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessCommand {
    Scrape,
    Crawl,
    Embed,
    Ingest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshnessRequest {
    pub command: FreshnessCommand,
    pub every_seconds: i64,
}
```

Update `crates/axon-core/src/config/types.rs`:

```rust
pub mod freshness;
```

Add to `Config`:

```rust
pub freshness: Option<super::freshness::FreshnessRequest>,
```

and set `freshness: None` in `Default`.

- [ ] **Step 4: Split scrape argument shapes**

`ScrapeArgs` is shared by `brand`, `summarize`, and `screenshot`, so do not add `fresh` to that shared type. In `crates/axon-core/src/config/cli.rs`, create:

```rust
#[derive(Debug, Args)]
pub(super) struct FreshScrapeArgs {
    pub(super) positional_urls: Vec<String>,

    /// Create or update a recurring freshness schedule, e.g. --fresh 1d.
    #[arg(long, value_parser = parse_fresh_arg)]
    pub(super) fresh: Option<FreshDuration>,
}

fn parse_fresh_arg(raw: &str) -> Result<FreshDuration, String> {
    FreshDuration::parse(raw)
}
```

Use `FreshScrapeArgs` only for `CliCommand::Scrape`. Add equivalent `fresh: Option<FreshDuration>` fields to `CrawlArgs`, `EmbedArgs`, and `IngestArgs`.

- [ ] **Step 5: Populate `Config::freshness` without panics**

Extend `DispatchOutput` with `freshness: Option<FreshnessRequest>`. In the supported command arms, map the parsed `FreshDuration` into a `FreshnessRequest`. Do not use `panic!`; invalid values must fail through clap value parsing.

- [ ] **Step 6: Run parser and help tests**

Run:

```bash
cargo test -p axon-core fresh_duration -- --nocapture
cargo test -p axon-core parse -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-core/src/config
git commit -m "feat(config): parse freshness schedules"
```

### Task 2: Safe Replay Snapshot and Versioned Payloads

**Files:**
- Create: `crates/axon-services/src/freshness.rs`
- Test: `crates/axon-services/src/freshness_tests.rs`

**Interfaces:**
- Consumes: `Config` and command-specific effective options.
- Produces: `FreshnessRequestV1`, `SafeReplayConfigV1`, `safe_replay_snapshot`, `freshness_identity_hash`.

- [ ] **Step 1: Write failing safe-snapshot tests**

Create `crates/axon-services/src/freshness_tests.rs`:

```rust
#[test]
fn safe_replay_snapshot_does_not_persist_secret_headers() {
    let mut cfg = Config::test_default();
    cfg.custom_headers = vec![
        ("Authorization".to_string(), "Bearer sk-secret".to_string()),
        ("Cookie".to_string(), "sid=secret".to_string()),
        ("X-Docs-Version".to_string(), "latest".to_string()),
    ];
    let err = safe_replay_snapshot(&cfg).unwrap_err();
    assert!(err.to_string().contains("secret-bearing headers cannot be stored in freshness schedules"));
}

#[test]
fn safe_replay_snapshot_strips_freshness_intent() {
    let mut cfg = Config::test_default();
    cfg.freshness = Some(FreshnessRequest {
        command: FreshnessCommand::Scrape,
        every_seconds: 86_400,
    });
    let snapshot = safe_replay_snapshot(&cfg).unwrap();
    assert!(snapshot.freshness_is_stripped);
}

#[test]
fn identity_hash_distinguishes_collection_and_render_mode() {
    let a = freshness_identity_hash("scrape", "https://example.com", 86_400, &json!({"url":"https://example.com"}), &json!({"collection":"a","render_mode":"http"}));
    let b = freshness_identity_hash("scrape", "https://example.com", 86_400, &json!({"url":"https://example.com"}), &json!({"collection":"b","render_mode":"http"}));
    assert_ne!(a, b);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-services freshness -- --nocapture`

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Add versioned payload DTOs**

In `crates/axon-services/src/freshness.rs`, define:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "schema_version", rename_all = "snake_case")]
pub enum FreshnessRequestPayload {
    V1(FreshnessRequestV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum FreshnessRequestV1 {
    Scrape { url: String },
    Crawl { urls: Vec<String> },
    Embed { input: String },
    Ingest { source: axon_jobs::ingest::IngestSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeReplayConfigV1 {
    pub collection: String,
    pub render_mode: String,
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub custom_headers: Vec<(String, String)>,
    pub embed: bool,
    pub freshness_is_stripped: bool,
}
```

- [ ] **Step 4: Implement safe replay snapshot**

Whitelist replay-affecting non-secret fields. Reject secret-bearing headers by case-insensitive header name: `authorization`, `cookie`, `x-api-key`, `proxy-authorization`, `set-cookie`. Do not store provider tokens or service URLs from the full `Config`. Clone the config intent with `freshness = None` before serialization.

- [ ] **Step 5: Add dispatch-time validation helpers**

Add `validate_freshness_payload_for_dispatch(payload, cfg)` and require:

- Scrape/crawl URLs pass existing URL validation.
- URL-valued embed inputs pass URL validation; local embed inputs pass the same server/local embed allowed-root validation used by MCP/embed service paths.
- RSS/feed ingest targets pass URL validation after source classification.
- Generic Git/Gitea/GitLab/GitHub targets use existing ingest source validation.

Add a test that a local embed schedule created for a safe temp file fails `run-now` after the file is replaced by a symlink pointing outside the allowed root.

- [ ] **Step 6: Run safe snapshot tests**

Run: `cargo test -p axon-services freshness -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-services/src/freshness.rs crates/axon-services/src/freshness_tests.rs
git commit -m "feat(freshness): define safe replay snapshots"
```

### Task 3: SQLite Freshness Storage and Leases

**Files:**
- Create: `crates/axon-jobs/src/freshness.rs`
- Create: `crates/axon-jobs/src/freshness/rows.rs`
- Create: `crates/axon-jobs/src/migrations/NNN_freshness.sql`
- Modify: `crates/axon-jobs/src/lib.rs`
- Test: `crates/axon-jobs/src/freshness_tests.rs`

**Interfaces:**
- Consumes: identity hash, versioned request JSON, safe config JSON.
- Produces: `FreshnessDef`, `FreshnessRun`, `create_freshness_def_with_pool`, `lease_due_freshness`, `lease_freshness_for_manual_run`, `heartbeat_freshness_run`, `finish_freshness_run_with_pool`, `reclaim_stale_freshness_leases`.

- [ ] **Step 1: Write failing storage tests**

Create `crates/axon-jobs/src/freshness_tests.rs`:

```rust
#[tokio::test]
async fn create_and_list_freshness_def_round_trips_safe_payload() {
    let pool = test_pool().await;
    let input = FreshnessDefCreate {
        name: "daily-mcp-spec".to_string(),
        command: "scrape".to_string(),
        target: "https://modelcontextprotocol.io/specification".to_string(),
        identity_hash: "hash-a".to_string(),
        request_json: serde_json::json!({"schema_version":"v1","command":"scrape","url":"https://modelcontextprotocol.io/specification"}),
        config_json: serde_json::json!({"collection":"axon-test","render_mode":"http"}),
        every_seconds: 86_400,
        enabled: true,
        next_run_at: chrono::Utc::now() + chrono::Duration::days(1),
    };
    let created = create_freshness_def_with_pool(&pool, &input).await.unwrap();
    let listed = list_freshness_defs_with_pool(&pool, 10).await.unwrap();
    assert_eq!(listed[0].id, created.id);
    assert_eq!(listed[0].identity_hash, "hash-a");
    assert_eq!(listed[0].request_json["command"], "scrape");
}

#[tokio::test]
async fn identity_hash_allows_same_target_in_different_collections() {
    let pool = test_pool().await;
    insert_freshness(&pool, "hash-prod", "scrape", "https://example.com", "prod").await;
    insert_freshness(&pool, "hash-test", "scrape", "https://example.com", "test").await;
    assert_eq!(list_freshness_defs_with_pool(&pool, 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn lease_due_freshness_is_single_flight_and_advances_next_run() {
    let pool = test_pool().await;
    let id = insert_due_freshness(&pool, "hash-a", "ingest", "rss:https://example.com/feed.xml").await;
    let first = lease_due_freshness(&pool, now_ms(), 300_000, 4).await.unwrap();
    let second = lease_due_freshness(&pool, now_ms(), 300_000, 4).await.unwrap();
    assert_eq!(first[0].id, id);
    assert!(second.is_empty());
    assert!(first[0].next_run_at.timestamp_millis() > now_ms());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-jobs freshness -- --nocapture`

Expected: FAIL because storage does not exist.

- [ ] **Step 3: Add migration**

Create `crates/axon-jobs/src/migrations/NNN_freshness.sql`:

```sql
CREATE TABLE IF NOT EXISTS axon_freshness_defs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  command TEXT NOT NULL,
  target TEXT NOT NULL,
  identity_hash TEXT NOT NULL UNIQUE,
  request_json TEXT NOT NULL,
  config_json TEXT NOT NULL,
  every_seconds INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  next_run_at INTEGER NOT NULL,
  lease_expires_at INTEGER,
  last_run_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_due
  ON axon_freshness_defs(enabled, next_run_at, lease_expires_at);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_target
  ON axon_freshness_defs(command, target);

CREATE TABLE IF NOT EXISTS axon_freshness_runs (
  id TEXT PRIMARY KEY,
  freshness_id TEXT NOT NULL,
  status TEXT NOT NULL,
  dispatched_job_id TEXT,
  error_text TEXT,
  result_json TEXT,
  started_at INTEGER,
  finished_at INTEGER,
  heartbeat_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(freshness_id) REFERENCES axon_freshness_defs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_runs_def_created
  ON axon_freshness_runs(freshness_id, created_at DESC);
```

- [ ] **Step 4: Implement storage with watch-grade recovery**

Implement in `crates/axon-jobs/src/freshness.rs`:

- `MIN_FRESHNESS_INTERVAL_SECS = 86_400`.
- `MAX_FRESHNESS_INTERVAL_SECS = 366 * 86_400`.
- `MAX_FRESHNESS_DEFS = 10_000`.
- `lease_due_freshness(pool, now, lease_ttl_ms, limit)` with `limit.clamp(1, 4)` for v1 default use.
- `lease_freshness_for_manual_run` that does not require `next_run_at <= now` but uses the same active-lease guard.
- `heartbeat_freshness_run` to extend `lease_expires_at` for long sync scrape runs.
- `reclaim_stale_freshness_leases` at runtime startup.
- Result/error persistence must redact secrets and cap `error_text` and `result_json` before writing.

- [ ] **Step 5: Add jitter helper**

Add a pure helper:

```rust
pub fn stable_initial_jitter_seconds(identity_hash: &str, every_seconds: i64) -> i64 {
    let max = std::cmp::min(3600, every_seconds / 10).max(1);
    let prefix = u64::from_str_radix(&identity_hash[..16], 16).unwrap_or(0);
    (prefix % max as u64) as i64
}
```

Creation must default `next_run_at = now + every_seconds + stable_jitter`.

Add a test that 100 schedule identities with the same interval get non-identical `next_run_at` offsets.

- [ ] **Step 6: Run storage tests**

Run: `cargo test -p axon-jobs freshness -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-jobs/src/freshness.rs crates/axon-jobs/src/freshness crates/axon-jobs/src/freshness_tests.rs crates/axon-jobs/src/migrations
git commit -m "feat(jobs): persist freshness schedules"
```

### Task 4: Service Orchestration and Backpressure

**Files:**
- Modify: `crates/axon-services/src/freshness.rs`
- Modify: `crates/axon-services/src/context.rs`
- Test: `crates/axon-services/src/freshness_tests.rs`

**Interfaces:**
- Consumes: `axon_jobs::freshness` lease/storage functions and `ServiceContext`.
- Produces: `spawn_freshness_scheduler`, `run_freshness_now`, `create_from_config`, `list`, `history`.

- [ ] **Step 1: Write failing orchestration tests**

Add tests proving:

```rust
#[tokio::test]
async fn scheduler_limits_concurrent_dispatches() {
    let max_seen = run_fake_freshness_scheduler_with_limits(20, 2).await;
    assert_eq!(max_seen, 2);
}

#[tokio::test]
async fn wait_true_uses_manual_lease_and_does_not_double_fire() {
    let outcome = create_then_run_now_with_fake_dispatcher().await;
    assert_eq!(outcome.run_count, 1);
    assert!(outcome.next_run_at > outcome.created_at);
}

#[tokio::test]
async fn stored_freshness_intent_is_not_replayed_recursively() {
    let outcome = dispatch_stored_schedule_with_snapshot().await;
    assert_eq!(outcome.created_schedules, 0);
    assert_eq!(outcome.executed_work, 1);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-services freshness -- --nocapture`

Expected: FAIL.

- [ ] **Step 3: Implement service-owned scheduler**

In `crates/axon-services/src/freshness.rs`, implement a scheduler loop that:

- Runs only from `ServiceContext::new_with_workers()`.
- Uses config/TOML/env values:
  - `freshness.tick_secs`, env `AXON_FRESHNESS_TICK_SECS`, default `60`.
  - `freshness.lease_secs`, env `AXON_FRESHNESS_LEASE_SECS`, default `1800`.
  - `freshness.max_due_per_tick`, env `AXON_FRESHNESS_MAX_DUE_PER_TICK`, default `4`.
  - `freshness.max_concurrent_runs`, env `AXON_FRESHNESS_MAX_CONCURRENT_RUNS`, default `2`.
- Holds a Tokio semaphore around every dispatch, including sync scrape.
- Uses no catch-up loop: on lease, advance `next_run_at` to `now + every_seconds`.

- [ ] **Step 4: Implement dispatch through existing services**

Dispatch rules:

- `ingest`: deserialize `FreshnessRequestV1::Ingest`, revalidate, call `ingest_start_with_context`. If an active `(source_type,target)` ingest job exists, record `skipped_active_job` in run history instead of duplicating work.
- `crawl`: deserialize `FreshnessRequestV1::Crawl`, revalidate, enqueue crawl through existing crawl service context. If an active crawl for the same seed cluster exists, record `skipped_active_job`.
- `embed`: deserialize `FreshnessRequestV1::Embed`, revalidate local path/URL at dispatch, enqueue embed through existing embed service context. If the local file is now a symlink outside allowed roots, fail visibly in run history.
- `scrape`: deserialize `FreshnessRequestV1::Scrape`, revalidate, run inside the freshness semaphore with heartbeat and timeout. Record completion/failure in `axon_freshness_runs`.

- [ ] **Step 5: Implement queue-cap and in-flight tests**

Add tests for crawl/embed/ingest dispatch when the relevant queue cap is full or an active target-equivalent job exists. Expected behavior: no duplicate enqueue; run history says `skipped_active_job` or contains the queue-cap error.

- [ ] **Step 6: Run orchestration tests**

Run:

```bash
cargo test -p axon-services freshness -- --nocapture
cargo test -p axon-jobs freshness -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-services/src/freshness.rs crates/axon-services/src/context.rs crates/axon-services/src/freshness_tests.rs
git commit -m "feat(freshness): run schedules through services"
```

### Task 5: CLI Fresh Command and Command Integration

**Files:**
- Modify: `crates/axon-core/src/config/types/enums.rs`
- Modify: `crates/axon-core/src/config/cli.rs`
- Modify: `crates/axon-core/src/config/parse/build_config/command_dispatch.rs`
- Modify: `crates/axon-cli/src/lib.rs`
- Modify: `crates/axon-cli/src/commands/mod.rs`
- Create: `crates/axon-cli/src/commands/fresh.rs`
- Modify: `crates/axon-cli/src/commands/{scrape,crawl,embed,ingest}.rs`
- Test: `crates/axon-cli/src/commands/fresh_tests.rs`

**Interfaces:**
- Consumes: service freshness functions.
- Produces: CLI schedule creation and `axon fresh list/run-now/history`.

- [ ] **Step 1: Write failing CLI tests**

Add tests for:

```text
axon scrape https://example.com --fresh 1d
axon crawl https://example.com/docs --fresh 7d
axon ingest rss:https://example.com/feed.xml --fresh 1d
axon fresh list --json
axon fresh run-now <id> --json
axon fresh history <id> --json
```

Creation output must include schedule id, command, target, interval, next run, and redacted safe payload. It must not include secret headers.

- [ ] **Step 2: Add `CommandKind::Fresh` and clap command**

Add `FreshArgs` with subcommands:

```rust
enum FreshSubcommand {
    List { #[arg(long)] json: bool },
    RunNow { id: String, #[arg(long)] json: bool },
    History { id: String, #[arg(long, default_value_t = 50)] limit: i64, #[arg(long)] json: bool },
}
```

- [ ] **Step 3: Integrate creation handlers**

In `run_scrape`, `run_crawl`, `run_embed`, and `run_ingest`, branch early when `cfg.freshness.is_some()`:

```rust
let schedule = freshness_service::create_from_config(cfg, service_context).await?;
emit_created(cfg, &schedule)?;
if cfg.wait {
    freshness_service::run_now(service_context, schedule.id).await?;
}
return Ok(());
```

Because creation sets `next_run_at = now + interval + jitter`, `--wait true` must use the same manual lease path as `axon fresh run-now`, not the normal command path.

- [ ] **Step 4: Run CLI tests**

Run:

```bash
cargo test -p axon-cli fresh -- --nocapture
cargo test -p axon-core cli_help -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-cli/src crates/axon-core/src/config
git commit -m "feat(cli): expose freshness schedules"
```

### Task 6: RSS Dedupe Regression Coverage

**Files:**
- Modify: `crates/axon-ingest/src/rss.rs`
- Modify: `crates/axon-ingest/src/rss_tests.rs`
- Inspect: `docs/reference/qdrant-payload-schema.md`

**Interfaces:**
- Consumes: RSS/Atom entry link, id, title, and content.
- Produces: stable no-duplicate behavior for repeated feed ingest.

- [ ] **Step 1: Write RSS dedupe tests**

Add tests proving:

```rust
#[test]
fn duplicate_entry_links_collapse_before_embedding() {
    let feed = parse(RSS_WITH_DUPLICATE_LINKS);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    let urls: Vec<_> = docs.iter().map(|doc| doc.url.as_str()).collect();
    assert_eq!(urls, vec!["https://example.com/a"]);
}

#[test]
fn duplicate_entry_links_use_normalized_url_identity() {
    let feed = parse(RSS_WITH_TRACKING_PARAM_VARIANTS);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    assert_eq!(docs.len(), 1);
}

#[test]
fn entry_identity_prefers_link_over_mutable_guid() {
    let feed = parse(RSS_WITH_LINK_AND_GUID);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    assert_eq!(docs[0].url, "https://example.com/a");
    assert_eq!(docs[0].extra["entry_id"], "guid-1");
}
```

- [ ] **Step 2: Run tests to discover current behavior**

Run: `cargo test -p axon-ingest rss -- --nocapture`

Expected: FAIL if duplicate links are currently emitted; PASS only if current behavior already collapses them.

- [ ] **Step 3: Add duplicate-entry filtering when Step 2 fails**

Inside `prepare_feed_docs`, maintain a `HashSet<String>` of normalized entry link identities. Use Axon’s existing URL normalization helper. Preserve the original canonical article URL in the document payload. Explicitly preserve meaningful query params unless the existing normalizer already strips known tracking params; document the chosen behavior in the test names.

- [ ] **Step 4: Confirm deterministic upsert docs**

Inspect `docs/reference/qdrant-payload-schema.md`. If it still accurately says point IDs derive from `(url, chunk_index)` and upsert overwrites, no docs edit is required. If implementation changes this, update the doc.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p axon-ingest rss -- --nocapture
cargo test -p axon-vector qdrant_store -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-ingest/src/rss.rs crates/axon-ingest/src/rss_tests.rs docs/reference/qdrant-payload-schema.md
git commit -m "test(rss): pin feed dedupe semantics"
```

### Task 7: Docs, Config, Smokes, and Follow-Up Beads

**Files:**
- Create: `docs/reference/actions/fresh.md`
- Modify: `docs/reference/actions/{scrape,crawl,embed,ingest,diff}.md`
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `.env.example`
- Modify: `config.example.toml`
- Modify: `docs/guides/configuration.md`
- Create: `scripts/smoke-freshness.sh`

**Interfaces:**
- Consumes: implemented freshness CLI.
- Produces: operator-facing docs and execution-proof smoke.

- [ ] **Step 1: Document CLI-only v1**

Add examples:

```bash
axon scrape https://modelcontextprotocol.io/specification --fresh 1d
axon crawl https://modelcontextprotocol.io/docs/getting-started/intro --fresh 1d
axon ingest unraid/api --fresh 7d
axon fresh list --json
axon fresh run-now <id> --json
axon fresh history <id> --json
```

Document that REST/MCP/web/palette freshness management is not in v1 and has follow-up beads.

- [ ] **Step 2: Document scheduler config**

Add TOML-first settings and env overrides:

```toml
[freshness]
tick-secs = 60
lease-secs = 1800
max-due-per-tick = 4
max-concurrent-runs = 2
run-retention-days = 90
```

Document env overrides:

```text
AXON_FRESHNESS_TICK_SECS
AXON_FRESHNESS_LEASE_SECS
AXON_FRESHNESS_MAX_DUE_PER_TICK
AXON_FRESHNESS_MAX_CONCURRENT_RUNS
AXON_FRESHNESS_RUN_RETENTION_DAYS
```

- [ ] **Step 3: Document existing `axon diff`**

Refresh `docs/reference/actions/diff.md` only if stale. State that two-URL compare is already implemented through CLI, MCP, REST, and service layers.

- [ ] **Step 4: Add isolated smoke**

Create `scripts/smoke-freshness.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

export AXON_DATA_DIR="$TMP/data"
export AXON_SQLITE_PATH="$TMP/jobs.db"
export AXON_FRESHNESS_TICK_SECS=1
export AXON_FRESHNESS_MAX_DUE_PER_TICK=2
export AXON_FRESHNESS_MAX_CONCURRENT_RUNS=1

COLLECTION="axon-freshness-smoke-$(date +%s)"
cd "$ROOT"

SCRAPE_JSON="$(./scripts/axon scrape https://example.com --collection "$COLLECTION" --fresh 1d --json)"
SCRAPE_ID="$(printf '%s' "$SCRAPE_JSON" | jq -r '.id')"
./scripts/axon fresh run-now "$SCRAPE_ID" --json | jq -e '.status == "completed" or .status == "enqueued"'

RSS_JSON="$(./scripts/axon ingest rss:https://github.com/jmagar/axon/releases.atom --collection "$COLLECTION" --fresh 1d --json)"
RSS_ID="$(printf '%s' "$RSS_JSON" | jq -r '.id')"
./scripts/axon fresh run-now "$RSS_ID" --json | jq -e '.status == "completed" or .status == "enqueued"'
./scripts/axon fresh history "$SCRAPE_ID" --json | jq -e '.items | length >= 1'
```

If the live GitHub feed is flaky or rate-limited in CI, replace it with a local fixture HTTP server and keep the public feed as a manual live smoke.

- [ ] **Step 5: Create follow-up beads**

Create follow-up beads for:

- REST/MCP freshness creation and management with auth inventory.
- Web/palette freshness UI.
- `fresh pause/resume/delete/update/get/artifacts`.
- Content-hash skip-before-embed for unchanged feeds/repos/docs.
- Dedicated scrape job family, if sync scrape freshness remains bounded but cancellation/status UX is insufficient.

- [ ] **Step 6: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
./scripts/smoke-freshness.sh
git diff --check
```

Expected: all pass. If a failure is pre-existing in this worktree, fix it before PR closeout.

- [ ] **Step 7: Commit**

```bash
git add docs README.md CLAUDE.md .env.example config.example.toml scripts
git commit -m "docs: document freshness schedules"
```

## Engineering Review Applied

- Dependency cycle avoided: no service dispatch from `axon-jobs`.
- Schedule identity fixed: `identity_hash` includes canonical request/config and interval.
- Lease recovery specified: heartbeat, stale reclaim, manual lease path, finalization visibility.
- Safe replay required: secret rejection/redaction, dispatch-time validation, bounded run payloads.
- V1 scoped to CLI-only, with REST/MCP/web/palette deferred.
- Scheduler hardened: low due batch, global semaphore, stable jitter, no catch-up burst.
- `--fresh --wait true` uses manual lease/run-now semantics.
- RSS dedupe expanded to normalized duplicate-entry coverage.
- Smokes now execute schedules in an isolated data dir and inspect run history.

## Failure Modes

| Codepath | Failure Mode | Rescued? | Test? | User Sees? | Logged? |
|---|---|---:|---:|---|---:|
| CLI parse | Invalid duration panics or gives unclear error | Y | Y | visible clap error | N/A |
| Snapshot | Secret header persisted in SQLite/history | Y | Y | creation rejected | Y |
| Storage | Same target/different collection collides | Y | Y | no collision | N/A |
| Lease | Crash leaves schedule stuck leased | Y | Y | stale lease reclaimed | Y |
| Scheduler | Restart fires huge due burst | Y | Y | bounded backlog | Y |
| Manual run | `run-now` races scheduler | Y | Y | one run | Y |
| Dispatch | Stored payload bypasses URL/local-path validation | Y | Y | run failure | Y |
| Scrape | Sync scrape bypasses all backpressure | Y | Y | bounded run | Y |
| RSS | Duplicate links embed twice | Y | Y | no duplicate docs | Y |
| Smoke | Creation passes but execution broken | Y | Y | smoke fails | Y |

## Not In Scope

- REST/MCP freshness management: deferred to avoid auth/OpenAPI/generated-client scope in v1.
- Web/palette freshness UI: deferred until CLI/service behavior is stable.
- Pause/resume/delete/update/get/artifacts: useful lifecycle controls, but not required for the first working scheduler.
- Content-hash skip-before-embed: valuable TEI/Qdrant optimization, but not required for correctness because deterministic upsert prevents duplicate points.
- Dedicated scrape job family: deferred if v1 bounded sync scrape proves reliable enough.

## Self-Review

Spec coverage:
- `--fresh <Nd>` on ingest/scrape/crawl/embed is covered by Tasks 1, 4, and 5.
- RSS dedupe is covered by Task 6.
- `axon diff` exists and docs refresh is covered by Task 7.
- Scheduler persistence, identity, single-flight execution, jitter, and backpressure are covered by Tasks 2, 3, and 4.
- Safe replay, SSRF/local-file validation, and redaction are covered by Tasks 2 and 4.
- Live smokes, clean linter, green tests, docs refresh, PR issue linkage, and CI expectations are covered by Task 7 and closeout.

Placeholder scan:
- No step says TBD, TODO, implement later, or "add tests" without concrete examples.

Type consistency:
- `FreshDuration`, `FreshnessRequest`, `FreshnessCommand`, `FreshnessRequestPayload`, `FreshnessRequestV1`, `SafeReplayConfigV1`, `FreshnessDef`, and `FreshnessRun` are introduced before later tasks consume them.

## Closeout Checklist

- [ ] Self-contained GitHub issue exists and includes the plan plus bead contents.
- [ ] PR links to the GitHub issue.
- [ ] All relevant beads are updated or closed.
- [ ] Follow-up beads exist for deferred work.
- [ ] Linter, tests, live smokes, generated checks, and docs staleness checks pass.
- [ ] CI is passing on the PR.
