# Provider Cooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 3B Task 3's remaining gap — wire the already-defined `ProviderCooling` type into job claim eligibility, so a saturated provider actually backs off instead of the job being re-claimed and re-failing in a hot loop.

**Architecture:** `axon_error::cooling::ProviderCooling` and `ApiError::with_provider_cooling`/`provider_cooling()` already exist and are already consumed by `axon_observe::reservation::cooling_snapshot` — this is not net-new plumbing, only the missing link between "an error carries a cooling window" and "the unified job store's claim query respects it." No `cooldown_until` column exists yet; cooling data currently only round-trips through the serialized error JSON blob written on a terminal/waiting status update.

**Tech Stack:** Rust 2024, `sqlx` SQLite, `axon-jobs`, `axon-error`.

## Global Constraints

- Split out of `2026-07-08-finish-job-cutover-and-security-completion.md` per engineering review: this work is independent of the crawl/embed/ingest job cutover and can land before, after, or in parallel with it.
- `ProviderCooling.cooldown_until` must be bounded — clamp any incoming value to a maximum window (e.g. 1 hour) so a buggy or malicious far-future timestamp cannot permanently blacklist a job kind from ever being claimed again. This was flagged as a DoS-shaped risk in engineering review and must not ship unbounded.
- `cooldown_until` must be cleared (`NULL`) on every transition to a non-`Waiting` status, not just left to expire — a job that cooled once and later completes must not retain a stale cooldown that silently blocks its next legitimate run.
- The new claim-query predicate must stay covered by an index — do not add an unindexed filter to a query that runs on every worker poll.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.

---

## Source-Of-Truth Contracts

- `docs/pipeline-unification/plans/2026-07-04-phase-3b-security-error-memory-completion.md` (Task 3)
- `docs/pipeline-unification/runtime/error-handling.md`

## Current-State Anchors

- `axon_error::cooling::ProviderCooling` — real type, already has consumers (`ApiError::with_provider_cooling`/`provider_cooling()`, `axon_observe::reservation::cooling_snapshot`). Read this file first to get the exact real field names/methods before writing any code against it — do not assume the illustrative names below are exact.
- Claim query and its existing index: `crates/axon-jobs/src/workers/unified.rs::claim_next_unified_job_unchecked` (or wherever the real claim SQL lives), `idx_axon_jobs_claim` in the migration that created it (search `crates/axon-jobs/src/migrations/` for the partial index keyed on `status`).
- Terminal/status-write path that must clear `cooldown_until`: `crates/axon-jobs/src/workers/unified.rs::mark_terminal` (or wherever the real terminal-status write function is).

## File Structure

- Modify: `crates/axon-jobs/src/migrations/` (new migration: `cooldown_until` column + covering index)
- Modify: `crates/axon-jobs/src/unified/control.rs` (or wherever `update_status`/terminal-write logic lives)
- Modify: `crates/axon-jobs/src/workers/unified.rs` (claim query predicate)
- Modify: real provider-saturation error call site (found in Task 1 below)
- Test: `crates/axon-jobs/src/provider_cooling_tests.rs`

---

## Task 1: Read The Real Cooling Type And Claim Query Before Writing Any Code

**Files:**
- Read only: `crates/axon-error/src/cooling.rs`, `crates/axon-error/src/api_error.rs`, `crates/axon-jobs/src/workers/unified.rs`, `crates/axon-jobs/src/migrations/`

**Interfaces:**
- Consumes: nothing — this is a research step.
- Produces: confirmed real names for `ProviderCooling`'s fields, `ApiError`'s cooling accessor, the claim query's exact SQL and index, and the exact call site where a provider-saturation error is currently raised after local retries are exhausted (search `crates/axon-services`, `crates/axon-embedding`, `crates/axon-llm` for existing 429/rate-limit handling — the root `CLAUDE.md`'s "TEI retries" section documents the retry policy this sits downstream of).

- [ ] **Step 1: Read and record the real API surface**

Read the four files above in full. Write down (as a comment at the top of the new test file created in Task 2) the exact real signatures for: `ProviderCooling::new`/`with_provider`/`with_reason` (or whatever the real constructors are), `ApiError::with_provider_cooling`/`provider_cooling()`, the claim query's table/column names, and `idx_axon_jobs_claim`'s exact `WHERE`/column list.

- [ ] **Step 2: Identify the real provider-saturation error call site**

Find where this repo currently detects a provider is saturated after exhausting local retries (e.g. TEI 429 after `TEI_MAX_RETRIES` attempts). Confirm whether it already returns an `ApiError` at all, or a different error type that would need converting.

## Task 2: Add `cooldown_until` Column And Covering Index

**Files:**
- Create: new migration under `crates/axon-jobs/src/migrations/`
- Test: `crates/axon-jobs/src/provider_cooling_tests.rs`

**Interfaces:**
- Consumes: Task 1's confirmed real table/column names.
- Produces: `cooldown_until INTEGER` (or the real timestamp representation this table already uses elsewhere — check whether other timestamp columns in this table are stored as epoch integers or RFC3339 strings and match that, since `ProviderCooling.cooldown_until` is a `DateTime<Utc>` and needs a consistent serialization on both the write and the claim-query read side) and an index covering the new claim predicate.

- [ ] **Step 1: Write a failing index-coverage test**

```rust
#[tokio::test]
async fn cooldown_until_column_and_index_exist() {
    let pool = crate::store::open_sqlite_pool(":memory:").await.unwrap();
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('jobs') WHERE name = 'cooldown_until'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(columns, vec!["cooldown_until".to_string()]);

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND sql LIKE '%cooldown_until%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !indexes.is_empty(),
        "expected an index covering cooldown_until, found none"
    );
}
```

Adjust the table name from `jobs` to whatever Task 1 Step 1 confirmed is real.

- [ ] **Step 2: Run test and confirm failure**

Run: `cargo test -p axon-jobs cooldown_until_column_and_index_exist --no-fail-fast`

Expected: FAIL.

- [ ] **Step 3: Write the migration**

Add the column and extend (or add a companion to) the existing claim index so the new predicate stays index-covered — do not add the column without also covering it, per the Global Constraints.

- [ ] **Step 4: Run test**

Run: `cargo test -p axon-jobs cooldown_until_column_and_index_exist --no-fail-fast`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-jobs/src/migrations
git commit -m "feat(jobs): add cooldown_until column with covering index"
```

## Task 3: Bound Cooling Windows And Wire Claim-Eligibility

**Files:**
- Modify: `crates/axon-jobs/src/unified/control.rs` (or the real status-update module found in Task 1)
- Modify: `crates/axon-jobs/src/workers/unified.rs`
- Test: `crates/axon-jobs/src/provider_cooling_tests.rs`

**Interfaces:**
- Consumes: Task 2's column/index.
- Produces: a `Waiting` transition carrying `ProviderCooling` writes a clamped `cooldown_until`; the claim query excludes rows whose `cooldown_until` is in the future; any subsequent non-`Waiting` status write clears it.

- [ ] **Step 1: Write a failing bounded-cooldown test**

```rust
#[tokio::test]
async fn cooldown_until_is_clamped_to_a_maximum_window() {
    let store = unified_store_fixture().await;
    let job = store.create(job_request_fixture("embed")).await.unwrap();
    let far_future = chrono::Utc::now() + chrono::Duration::days(365);
    let cooling = axon_error::cooling::ProviderCooling::new(far_future).with_provider("tei");

    transition_to_waiting_with_cooling(&store, job.job_id, cooling).await.unwrap();

    let stored = store.get(job.job_id).await.unwrap().unwrap();
    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until <= chrono::Utc::now() + chrono::Duration::hours(1) + chrono::Duration::minutes(1),
        "cooldown_until must be clamped to the configured maximum window, got {cooldown_until:?}"
    );
}
```

Adjust `transition_to_waiting_with_cooling`/`stored_cooldown_until` to whatever the real status-update API from Task 1 turns out to be.

- [ ] **Step 2: Write a failing claim-eligibility test**

```rust
#[tokio::test]
async fn cooling_job_is_not_claimable_before_cooldown_expires() {
    let store = unified_store_fixture().await;
    let job = store.create(job_request_fixture("embed")).await.unwrap();
    let cooldown_until = chrono::Utc::now() + chrono::Duration::seconds(30);
    let cooling = axon_error::cooling::ProviderCooling::new(cooldown_until).with_provider("tei");
    transition_to_waiting_with_cooling(&store, job.job_id, cooling).await.unwrap();

    let claimed = claim_next_unified_job_unchecked(&store.pool()).await.unwrap();
    assert!(
        claimed.is_none(),
        "a job in its cooling window must not be claimable"
    );
}
```

- [ ] **Step 3: Write a failing clear-on-terminal test**

```rust
#[tokio::test]
async fn cooldown_until_is_cleared_on_completion() {
    let store = unified_store_fixture().await;
    let job = store.create(job_request_fixture("embed")).await.unwrap();
    let cooling = axon_error::cooling::ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    transition_to_waiting_with_cooling(&store, job.job_id, cooling).await.unwrap();
    mark_terminal_completed(&store, job.job_id).await.unwrap();

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(cooldown_until.is_none(), "cooldown_until must be cleared once the job leaves Waiting");
}
```

- [ ] **Step 4: Run tests and confirm failure**

Run: `cargo test -p axon-jobs cooldown_until_is_clamped cooling_job_is_not_claimable cooldown_until_is_cleared --no-fail-fast`

Expected: all FAIL.

- [ ] **Step 5: Implement clamping on write**

In the status-update function that handles a `Waiting` transition with a `ProviderCooling`-bearing error, clamp `cooling.cooldown_until` to `min(cooling.cooldown_until, now + MAX_COOLDOWN_WINDOW)` before persisting, where `MAX_COOLDOWN_WINDOW` is a `const Duration` (start at 1 hour; do not make this configurable in this task — a fixed conservative bound is the point).

- [ ] **Step 6: Implement claim-query exclusion**

Add the `AND (cooldown_until IS NULL OR cooldown_until <= ?now)` predicate to the claim query, using the index added in Task 2.

- [ ] **Step 7: Implement clear-on-non-waiting**

In the terminal/generic status-write function, explicitly set `cooldown_until = NULL` whenever the new status is anything other than `Waiting`.

- [ ] **Step 8: Wire the real provider-saturation call site**

At the call site found in Task 1 Step 2, attach `ProviderCooling::new(cooldown_until).with_provider(provider_id)` to the `ApiError` before it propagates to the job runner's `Result<(), ApiError>`.

- [ ] **Step 9: Run tests**

```bash
cargo test -p axon-jobs provider_cooling --no-fail-fast
cargo test -p axon-jobs unified --no-fail-fast
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/axon-jobs/src crates/axon-services/src
git commit -m "feat(jobs): bound and wire provider cooling into job claim eligibility"
```

## Task 4: Verification

- [ ] **Step 1: Full crate gate**

```bash
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-error --no-fail-fast
cargo clippy -p axon-jobs -p axon-error --all-targets
```

Expected: PASS.

- [ ] **Step 2: Update the source plan doc**

Mark Phase 3B Task 3 done in `2026-07-04-phase-3b-security-error-memory-completion.md` with an evidence note pointing at this plan's commits.

- [ ] **Step 3: Commit**

```bash
git add docs/pipeline-unification/plans
git commit -m "docs(pipeline): close out phase 3b task 3 provider cooling"
```

## Self-Review

- Spec coverage: Phase 3B Task 3 → Tasks 1-3 here.
- Engineering review findings applied: bounded cooldown window (DoS fix), covering index added in the same migration as the column (performance fix), explicit clear-on-terminal (correctness fix for the stale-clobber risk architecture review flagged).
- Placeholder scan: Task 1 is an explicit read-first step precisely because the plan that spawned this one was caught inventing unverified API names — every downstream task depends on Task 1's findings rather than guessing.
