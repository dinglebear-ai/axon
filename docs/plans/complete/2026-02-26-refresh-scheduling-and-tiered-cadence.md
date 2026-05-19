# Refresh Scheduling And Tiered Cadence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add first-class scheduling for `refresh` so known URLs are revalidated automatically on tiered intervals (high/medium/low churn) while `crawl` remains separate for discovery.

**Architecture:** Build a DB-backed schedule registry and a scheduler worker loop that claims due schedules, resolves target URLs (from manifest or explicit list), and enqueues `refresh` jobs. Keep scheduling concerns isolated from refresh execution so refresh remains usable manually (`refresh --wait true`) and asynchronously.

**Tech Stack:** Rust (Tokio, SQLx, clap), existing Axon jobs framework (`run_job_worker`, Postgres tables, AMQP enqueue), existing refresh command/jobs implementation.

---

### Task 1: Add Schedule Data Model and Persistence API

**Files:**
- Modify: `crates/jobs/refresh.rs`
- Test: `crates/jobs/refresh.rs` (new `#[cfg(test)]` block)

**Step 1: Write failing test for schedule table creation**

```rust
#[tokio::test]
async fn ensure_schema_creates_refresh_schedule_table() {
    // use temporary pg test pool helper pattern from jobs/common/tests
    // query information_schema.tables for axon_refresh_schedules
    // assert table exists
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ensure_schema_creates_refresh_schedule_table -- --nocapture`
Expected: FAIL because `axon_refresh_schedules` is not created.

**Step 3: Add schedule schema + structs**

In `crates/jobs/refresh.rs`, add:
- SQL table creation in `ensure_schema`:

```sql
CREATE TABLE IF NOT EXISTS axon_refresh_schedules (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  seed_url TEXT,
  urls_json JSONB,
  every_seconds BIGINT NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT TRUE,
  next_run_at TIMESTAMPTZ NOT NULL,
  last_run_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
```

- Partial index for due schedules:

```sql
CREATE INDEX IF NOT EXISTS idx_axon_refresh_schedules_due
ON axon_refresh_schedules(next_run_at ASC)
WHERE enabled = TRUE
```

- Rust structs:
  - `RefreshSchedule`
  - `RefreshScheduleCreate`

**Step 4: Add persistence methods (minimal API)**

Add functions in `crates/jobs/refresh.rs`:
- `create_refresh_schedule(...)`
- `list_refresh_schedules(...)`
- `delete_refresh_schedule(...)`
- `set_refresh_schedule_enabled(...)`
- `claim_due_refresh_schedules(...)` (select-for-update skip locked)
- `mark_refresh_schedule_ran(...)`

**Step 5: Run test to verify pass**

Run: `cargo test ensure_schema_creates_refresh_schedule_table -- --nocapture`
Expected: PASS.

**Step 6: Commit**

```bash
git add crates/jobs/refresh.rs
git commit -m "feat(refresh): add schedule schema and persistence primitives"
```

---

### Task 2: Add CLI Surface for Schedule Management

**Files:**
- Modify: `crates/core/config/cli.rs`
- Modify: `crates/core/config/parse.rs`
- Modify: `crates/core/config/types.rs`
- Modify: `crates/cli/commands/refresh.rs`

**Step 1: Write failing parse test for refresh schedule subcommands**

Add in `crates/core/config/parse.rs` tests:

```rust
#[test]
fn parse_refresh_schedule_add_maps_positional_tokens() {
    // parse argv: axon refresh schedule add docs-medium https://docs.rs --every-seconds 21600
    // assert cfg.command == CommandKind::Refresh
    // assert cfg.positional starts with ["schedule","add",...]
}
```

**Step 2: Run test to verify fail**

Run: `cargo test parse_refresh_schedule_add_maps_positional_tokens -- --nocapture`
Expected: FAIL because subcommand not defined.

**Step 3: Add clap subcommands**

In `crates/core/config/cli.rs`, add under `refresh` command:
- `schedule add <name> [seed_url] --every-seconds <n> [--urls <csv>]`
- `schedule list`
- `schedule enable <name>`
- `schedule disable <name>`
- `schedule delete <name>`
- `schedule run-due`

Keep existing job subcommands unchanged.

**Step 4: Update parse wiring**

In `crates/core/config/parse.rs`:
- map schedule operations into `cfg.positional` tokens (same pattern as existing job subcommands).
- preserve existing refresh behavior for URL-based run.

**Step 5: Add refresh command handlers**

In `crates/cli/commands/refresh.rs`:
- branch on `cfg.positional[0] == "schedule"` and dispatch to new handlers.
- handlers call new persistence methods from `crates/jobs/refresh.rs`.

**Step 6: Run test and compile**

Run:
- `cargo test parse_refresh_schedule_add_maps_positional_tokens -- --nocapture`
- `cargo check -q`
Expected: PASS.

**Step 7: Commit**

```bash
git add crates/core/config/cli.rs crates/core/config/parse.rs crates/cli/commands/refresh.rs crates/core/config/types.rs
git commit -m "feat(refresh): add schedule CLI and parse routing"
```

---

### Task 3: Implement Tiered Cadence Presets (High/Medium/Low)

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/core/config/cli.rs`
- Test: `crates/cli/commands/refresh.rs`

**Step 1: Write failing unit test for tier mapping**

```rust
#[test]
fn refresh_tier_maps_to_expected_seconds() {
    assert_eq!(tier_to_seconds("high"), Some(1800));
    assert_eq!(tier_to_seconds("medium"), Some(21600));
    assert_eq!(tier_to_seconds("low"), Some(86400));
}
```

**Step 2: Run test to verify fail**

Run: `cargo test refresh_tier_maps_to_expected_seconds -- --nocapture`
Expected: FAIL.

**Step 3: Add tier flag and mapping**

Add optional schedule-add flag:
- `--tier <high|medium|low>`

Behavior:
- If `--every-seconds` provided: use it.
- Else if `--tier` provided: map to seconds.
- Else default to `medium` (`21600`).

**Step 4: Validate flag exclusivity**

Ensure clap disallows invalid combinations if needed (or explicit runtime validation message).

**Step 5: Run test and compile**

Run:
- `cargo test refresh_tier_maps_to_expected_seconds -- --nocapture`
- `cargo check -q`
Expected: PASS.

**Step 6: Commit**

```bash
git add crates/core/config/cli.rs crates/cli/commands/refresh.rs
git commit -m "feat(refresh): add tiered cadence presets for schedule creation"
```

---

### Task 4: Implement Scheduler Execution Path (`schedule run-due`)

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/jobs/refresh.rs`
- Test: `crates/jobs/refresh.rs` and `crates/cli/commands/refresh.rs`

**Step 1: Write failing test for claiming due schedules only**

```rust
#[tokio::test]
async fn claim_due_refresh_schedules_only_returns_enabled_due_rows() {
    // insert: due+enabled, future+enabled, due+disabled
    // assert only due+enabled is claimed
}
```

**Step 2: Run test to verify fail**

Run: `cargo test claim_due_refresh_schedules_only_returns_enabled_due_rows -- --nocapture`
Expected: FAIL.

**Step 3: Implement claim + dispatch logic**

In `crates/cli/commands/refresh.rs` add `handle_refresh_schedule_run_due`:
1. claim due schedules (bounded batch, e.g. 25).
2. for each schedule:
   - resolve URLs from explicit `urls_json` or from `seed_url` manifest (same logic as `resolve_refresh_urls`).
   - enqueue refresh job via `start_refresh_job`.
   - update schedule next run via `mark_refresh_schedule_ran(now + every_seconds)`.
3. produce JSON or human summary.

**Step 4: Add failing test for URL resolution fallback**

```rust
#[tokio::test]
async fn schedule_run_due_uses_seed_manifest_when_urls_missing() {
    // create schedule with seed_url only
    // create manifest file with urls
    // assert run-due enqueues refresh for manifest urls
}
```

**Step 5: Implement minimal code to pass test**

Reuse existing `urls_from_manifest_seed` behavior to avoid duplication.

**Step 6: Run tests and compile**

Run:
- `cargo test claim_due_refresh_schedules_only_returns_enabled_due_rows -- --nocapture`
- `cargo test schedule_run_due_uses_seed_manifest_when_urls_missing -- --nocapture`
- `cargo check -q`
Expected: PASS.

**Step 7: Commit**

```bash
git add crates/jobs/refresh.rs crates/cli/commands/refresh.rs
git commit -m "feat(refresh): implement due-schedule execution and refresh enqueue"
```

---

### Task 5: Add Long-Running Scheduler Worker Mode

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/jobs/refresh.rs`

**Step 1: Write failing test for scheduler tick interval default**

```rust
#[test]
fn refresh_schedule_worker_default_tick_is_30_seconds() {
    assert_eq!(refresh_schedule_tick_secs_default(), 30);
}
```

**Step 2: Run test to verify fail**

Run: `cargo test refresh_schedule_worker_default_tick_is_30_seconds -- --nocapture`
Expected: FAIL.

**Step 3: Implement `refresh schedule worker`**

Add handler:
- infinite loop:
  - run `schedule run-due`
  - sleep tick (default 30s, env override `AXON_REFRESH_SCHEDULER_TICK_SECS`)
- graceful logging around each sweep.

This worker is separate from `refresh worker` (which processes queued refresh jobs).

**Step 4: Run test and compile**

Run:
- `cargo test refresh_schedule_worker_default_tick_is_30_seconds -- --nocapture`
- `cargo check -q`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/cli/commands/refresh.rs crates/jobs/refresh.rs
git commit -m "feat(refresh): add schedule worker loop for periodic due checks"
```

---

### Task 6: Integrate Status Visibility for Refresh Jobs

**Files:**
- Modify: `crates/cli/commands/status.rs`
- Modify: `crates/jobs/refresh.rs`

**Step 1: Write failing status test for refresh section presence**

Add a test in `crates/cli/commands/status.rs`:

```rust
#[test]
fn status_snapshot_includes_refresh_jobs_key() {
    // assert serialized payload contains local_refresh_jobs
}
```

**Step 2: Run test to verify fail**

Run: `cargo test status_snapshot_includes_refresh_jobs_key -- --nocapture`
Expected: FAIL.

**Step 3: Add refresh jobs to status output**

In `crates/cli/commands/status.rs`:
- fetch `list_refresh_jobs(cfg, 20)` alongside existing sections.
- include `local_refresh_jobs` in JSON snapshot.
- add human-readable `Refresh` section summary.

**Step 4: Run test and compile**

Run:
- `cargo test status_snapshot_includes_refresh_jobs_key -- --nocapture`
- `cargo check -q`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/cli/commands/status.rs crates/jobs/refresh.rs
git commit -m "feat(status): include refresh jobs in status output"
```

---

### Task 7: Add Docs for Tiered Scheduling and Operations

**Files:**
- Modify: `README.md`
- Create: `docs/commands/refresh.md`

**Step 1: Write docs coverage check command**

Run:

```bash
rg -n "refresh schedule|tier|run-due|refresh worker" README.md docs/commands || true
```

Expected: missing/partial references before edit.

**Step 2: Document command usage in `docs/commands/refresh.md`**

Include:
- one-off refresh examples.
- schedule add/list/enable/disable/delete examples.
- tier presets table (`high=1800`, `medium=21600`, `low=86400`).
- distinction:
  - `refresh schedule worker` (scheduler)
  - `refresh worker` (job consumer)

**Step 3: Update README command matrix and “freshness strategy” section**

Add recommended production strategy:
- refresh high/medium/low tiers
- crawl daily/weekly for discovery only

**Step 4: Re-run docs coverage check**

Run:

```bash
rg -n "refresh schedule|tier|run-due|refresh worker" README.md docs/commands/refresh.md
```

Expected: all sections found.

**Step 5: Commit**

```bash
git add README.md docs/commands/refresh.md
git commit -m "docs(refresh): add scheduling workflow and tiered cadence guidance"
```

---

### Task 8: Final Verification and Live Smoke Test

**Files:**
- No new files (verification only)

**Step 1: Run focused tests**

```bash
cargo test refresh -- --nocapture
cargo test status_snapshot_includes_refresh_jobs_key -- --nocapture
cargo check -q
```

Expected: PASS.

**Step 2: Run formatter and lints**

```bash
cargo fmt --check
cargo clippy
```

Expected: PASS.

**Step 3: Live CLI smoke (local)**

```bash
./scripts/axon refresh schedule add docs-medium https://example.com --tier medium
./scripts/axon refresh schedule list --json
./scripts/axon refresh schedule run-due --json
./scripts/axon refresh list --json
./scripts/axon status --json
```

Expected:
- schedule row created.
- run-due enqueues at least one refresh job when due.
- refresh job appears in `refresh list` and `status` payload.

**Step 4: Optional long-running smoke**

In one terminal:

```bash
./scripts/axon refresh worker
```

In another terminal:

```bash
./scripts/axon refresh schedule worker
```

Expected:
- scheduler periodically enqueues due refresh jobs.
- refresh worker consumes and completes them.

**Step 5: Commit verification-only fixes (if any)**

```bash
git add <files-fixed-during-verification>
git commit -m "fix(refresh): address verification issues in scheduling flow"
```

---

## Non-Goals (explicit)
- Do not merge scheduler and refresh execution workers into one process.
- Do not replace crawl discovery with refresh scheduling.
- Do not add external scheduler service dependencies in this phase.

## Rollout Order
1. Deploy code with schedule tables + CLI.
2. Create initial schedules for 2-3 domains.
3. Run scheduler worker + refresh worker in staging.
4. Observe 24h run behavior.
5. Promote to production and expand schedules by tier.
