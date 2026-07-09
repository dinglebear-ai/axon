# Full Durable Job Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Axon's family-specific job persistence with one durable job model for all job-backed operations, while keeping normal synchronous read paths jobless.

**Architecture:** `axon-jobs` becomes the single lifecycle owner through `JobStore`, unified SQLite tables, attempts, stages, heartbeats, append-only events, artifacts, retention, and recovery. `axon-services` routes every async/detached operation through the unified job runtime and exposes one transport-neutral jobs service to CLI, MCP, REST/SSE, and web. Legacy crawl/embed/extract/ingest/watch tables are not migrated or backfilled; non-empty incompatible stores block unified workers until reset/preflight receipts prove a clean cutover.

**Tech Stack:** Rust 2024, Tokio, `sqlx` SQLite/WAL, `axon-api::source` job DTOs, `axon-jobs`, `axon-services`, `axon-cli`, `axon-mcp`, `axon-web`, `axon-observe`, source pipeline contracts in `docs/pipeline-unification/runtime/job-contract.md`.

## Global Constraints

- This is a full cutover, not a minimum slice.
- One durable jobs table family owns lifecycle for every long-running operation.
- Job-backed operations: detached/long-running source acquisition, watch, extraction/research/provider work, memory compaction/import, graph mutation, prune, provider_probe, and reset.
- Synchronous read paths such as normal `query` and `retrieve` stay jobless unless they perform long-running provider/artifact work.
- Every async or detached operation returns a `JobDescriptor`.
- Foreground CLI operations still create a job row when they perform source acquisition, embedding, graph mutation, pruning, extraction, research, or long-running provider work.
- Full status model: `queued`, `pending`, `running`, `waiting`, `blocked`, `canceling`, `completed`, `completed_degraded`, `failed`, `canceled`, `expired`, and `skipped`.
- Invalid status transitions fail without mutating job state.
- Required job fields include `auth_snapshot`, `config_snapshot_id`, `stage_plan`, `requirements`, `result_schema`, parent/root job ids, attempt number, warnings, and current/terminal `ApiError`.
- Events are append-only with monotonic per-job `sequence` and resumable `after_sequence` cursors.
- Job list/status/events must be O(page size), not O(total jobs/events).
- Panic guard, cancellation, recovery, heartbeat, stale reclaim, retry semantics, and same-`job_id` retry/recovery must be preserved before removing legacy readers.
- Stale recovery must not double-run provider-heavy stages or double-publish generations while the original attempt is still alive.
- Logs/traces parity can follow only if CLI/MCP/REST status correctness does not require it.
- Do not build legacy job-row migration, compatibility views, or read-only legacy parity. The clean-slate cutover contract requires incompatible-store detection plus reset/preflight receipts instead.
- Job event sequence allocation must be O(1): store and transactionally increment `last_event_sequence` on the job or attempt row. Do not use `MAX(sequence) + 1`.
- Job list and event cursor tests must prove O(page size) behavior with `EXPLAIN QUERY PLAN`, not only index-name existence.
- Typed `AuthSnapshot` is required in the durable job DTO/store task. Do not introduce raw `MetadataMap auth_snapshot` as an intermediate public shape.
- Split implementation order: core source/watch/reset/prune job model first; research/memory/graph/provider polish and web rendering follow after the unified model is proven.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.

---

## Scope Exception: `reset` Stays Jobless

Line 15 above and `OperationKind::Reset`/`job_policy_for_operation` (Task 1)
classify `reset` as job-backed by default. In practice `axon reset` is
implemented and shipped (`crates/axon-services/src/reset.rs`) as an
intentionally jobless, pre-runtime admin operation, and that is correct — not
a gap to close later:

- `reset`'s **default** mode is a dry-run, and its own tested contract
  (`crates/axon-services/src/reset_tests.rs::dry_run_reset_mutates_nothing_and_reports_plan`)
  requires that a dry-run touch nothing, including never creating the SQLite
  DB file if it does not already exist. Any job-backed tracking path
  (`enqueue_operation`/`start_operation_job`/`complete_operation_job`, or a
  bare `SqliteUnifiedJobStore` construction) opens and migrates the SQLite DB
  as a side effect of writing a job row, which would violate that invariant
  on the common (dry-run) path.
- `crates/axon-cli/src/lib.rs` deliberately runs `reset` *before* any
  `ServiceContext` (and therefore any job store) is constructed, precisely
  because constructing that runtime opens/migrates the DB. This is documented
  in-line at the `CommandKind::Reset` branch.
- `reset` can also select `RESET_STORE_JOBS` itself (see `SQLITE_STORES` in
  `reset.rs`), which wipes and re-migrates the same unified SQLite DB that
  would hold the job row. A job row created immediately before that wipe does
  not survive it — the wipe recreates a fresh empty schema — so tracking would
  be a non-durable artifact on exactly the runs where the plan's job-backed
  guarantee matters most.
- `reset` already has its own audit trail that serves the same purpose a job
  row would: the `ResetResult`/receipt written by
  `crates/axon-services/src/reset/artifacts.rs`, which is durable at the
  filesystem level and is not lost by the very wipe it documents.

Conclusion: `reset` is exempted from job-backed tracking. `OperationKind::Reset`
and its classification in `job_policy_for_operation` remain in the DTO/schema
for forward compatibility with `docs/pipeline-unification/foundation/types/*`
consumers that enumerate `OperationKind`, but no caller wires
`enqueue_operation`/`start_operation_job`/`complete_operation_job` for it, and
Task 6's "every job-backed operation creates a unified job descriptor" test
should exclude `OperationKind::Reset` (or assert it is jobless) rather than
assert it produces a `JobDescriptor`. Revisit only if `reset` is redesigned to
run after a durable job store is guaranteed to already exist and dry-run's
"mutate nothing" contract is relaxed or moved to a non-SQLite check.

---

## Engineering Review Corrections

Remove or rewrite any task that imports legacy job rows or preserves old family job IDs. The replacement is:

```text
detect incompatible non-empty legacy stores
block unified workers before side effects
emit reset/preflight guidance and receipts
initialize fresh unified schema only after explicit reset or developer override
```

Transport exposure should stage in this order: service/CLI proof, REST/MCP proof, then web rendering. Do not require all transports and all job kinds in one implementation PR.

## Source-Of-Truth Contracts

- `docs/pipeline-unification/runtime/job-contract.md`
- `docs/pipeline-unification/runtime/observability-contract.md`
- `docs/pipeline-unification/runtime/auth-contract.md`
- `docs/pipeline-unification/runtime/security-contract.md`
- `docs/pipeline-unification/surfaces/command-contract.md`
- `docs/pipeline-unification/surfaces/tool-contract.md`
- `docs/pipeline-unification/surfaces/rest-contract.md`
- `docs/pipeline-unification/schemas/database-schema.md`
- `docs/pipeline-unification/delivery/cutover-contract.md`
- `docs/pipeline-unification/delivery/testing-contract.md`

## Current-State Anchors

- Current family-specific backend: `crates/axon-jobs/src/backend.rs`
- Current SQLite runtime: `crates/axon-jobs/src/runtime.rs`
- Target store boundary: `crates/axon-jobs/src/boundary.rs`
- Existing unified store shell: `crates/axon-jobs/src/unified.rs`
- Existing state-machine helper: `crates/axon-jobs/src/state_machine.rs`
- Current service job facade still using family `JobKind`: `crates/axon-services/src/jobs.rs`
- Current family workers: `crates/axon-jobs/src/workers.rs` and `crates/axon-jobs/src/workers/**`
- REST jobs contract: `docs/pipeline-unification/surfaces/rest-contract.md`
- MCP jobs contract: `docs/pipeline-unification/surfaces/tool-contract.md`
- CLI jobs contract: `docs/pipeline-unification/surfaces/command-contract.md`

## File Structure

- Create: `crates/axon-api/src/source/job_policy.rs`
  - Operation classification: job-backed vs synchronous.
- Modify: `crates/axon-api/src/source/job.rs` or existing job DTO module
  - Add/complete `JobKind`, `JobIntent`, `JobDescriptor`, `JobSummary`, `JobCreateRequest`, `JobStatusUpdate`, `JobEventListRequest`, `JobEventPage`, `JobRecoveryRequest`, and related DTOs.
- Modify: `crates/axon-jobs/src/state_machine.rs`
  - Contract-complete transition validation.
- Create: `crates/axon-jobs/src/unified/schema.rs`
  - Unified SQLite table names, indexes, and SQL helpers.
- Modify: `crates/axon-jobs/src/migrations.rs`
  - Add migrations for unified tables and indexes only. Do not add legacy job import watermarks or compatibility views.
- Modify: `crates/axon-jobs/src/unified/{control.rs,ops.rs,heartbeat.rs,observe.rs}`
  - Make unified store authoritative for create/get/list/events/cancel/retry/recover/cleanup/artifacts/reset.
- Create: `crates/axon-jobs/src/unified/pagination.rs`
  - Cursor encoding/decoding for job and event pages.
- Create: `crates/axon-jobs/src/unified/retention.rs`
  - Terminal job/event retention pruning.
- Modify/create: `crates/axon-jobs/src/store_inventory.rs`
  - Detect incompatible non-empty legacy job tables and block unified workers until reset/preflight receipts exist.
- Modify: `crates/axon-jobs/src/workers.rs` and `crates/axon-jobs/src/workers/**`
  - Poll unified jobs by `job_kind` and stage plan; preserve panic guard, heartbeat, cancellation, retry, and stale reclaim.
- Modify: `crates/axon-services/src/jobs.rs`
  - Replace family `JobKind` facade with transport-neutral unified jobs service.
- Modify: `crates/axon-services/src/runtime.rs`
  - Enqueue job-backed operations through unified `JobStore`.
- Modify: source/watch/extract/research/memory/graph/prune/provider/reset services
  - Use job-backed classification and return `JobDescriptor` for async/detached work.
- Modify: `crates/axon-cli/src/commands/jobs.rs` or current jobs command module
  - Add unified jobs list/get/events/stream/cancel/retry/recover/cleanup/clear rendering.
- Modify: `crates/axon-mcp/src/**`
  - Route `action=jobs` subactions to unified jobs service.
- Modify: `crates/axon-web/src/**`
  - Add `/v1/jobs`, `/v1/jobs/{job_id}`, `/events`, `/stream`, `/cancel`, `/retry`, `/recover`, `/cleanup`, `/artifacts`.
- Modify tests in `crates/axon-jobs`, `crates/axon-services`, `crates/axon-cli`, `crates/axon-mcp`, and `crates/axon-web`.

## Task 1: Define Job-Backed Operation Policy

**Files:**

- Create: `crates/axon-api/src/source/job_policy.rs`
- Modify: `crates/axon-api/src/source/mod.rs`
- Test: `crates/axon-api/src/source_job_policy_tests.rs`

**Interfaces:**

- Consumes: canonical operation names and `JobExecutionMode`.
- Produces: `job_policy_for_operation(operation: OperationKind, mode: JobExecutionMode) -> JobPolicy`.

- [ ] **Step 1: Write failing classification tests**

Add `crates/axon-api/src/source_job_policy_tests.rs`:

```rust
use crate::source::{job_policy_for_operation, JobExecutionMode, JobPolicy, OperationKind};

#[test]
fn source_watch_extract_research_memory_graph_prune_provider_reset_are_job_backed() {
    for operation in [
        OperationKind::Source,
        OperationKind::Watch,
        OperationKind::Extract,
        OperationKind::Research,
        OperationKind::MemoryCompaction,
        OperationKind::MemoryImport,
        OperationKind::GraphMutation,
        OperationKind::Prune,
        OperationKind::ProviderProbe,
        OperationKind::Reset,
    ] {
        let policy = job_policy_for_operation(operation, JobExecutionMode::Detached);
        assert_eq!(policy, JobPolicy::JobBacked);
    }
}

#[test]
fn normal_query_and_retrieve_remain_jobless_until_long_running_work_is_requested() {
    assert_eq!(
        job_policy_for_operation(OperationKind::Query, JobExecutionMode::Foreground),
        JobPolicy::Synchronous
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Retrieve, JobExecutionMode::Foreground),
        JobPolicy::Synchronous
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Query, JobExecutionMode::LongRunningProvider),
        JobPolicy::JobBacked
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Retrieve, JobExecutionMode::ArtifactBacked),
        JobPolicy::JobBacked
    );
}
```

- [ ] **Step 2: Run the test and confirm failure**

Run:

```bash
cargo test -p axon-api source_job_policy --no-fail-fast
```

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement operation policy**

Create `job_policy.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Source,
    Watch,
    Extract,
    Research,
    MemoryCompaction,
    MemoryImport,
    GraphMutation,
    Prune,
    ProviderProbe,
    Reset,
    Query,
    Retrieve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobExecutionMode {
    Foreground,
    Detached,
    LongRunningProvider,
    ArtifactBacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobPolicy {
    JobBacked,
    Synchronous,
}

pub fn job_policy_for_operation(operation: OperationKind, mode: JobExecutionMode) -> JobPolicy {
    match operation {
        OperationKind::Source
        | OperationKind::Watch
        | OperationKind::Extract
        | OperationKind::Research
        | OperationKind::MemoryCompaction
        | OperationKind::MemoryImport
        | OperationKind::GraphMutation
        | OperationKind::Prune
        | OperationKind::ProviderProbe
        | OperationKind::Reset => JobPolicy::JobBacked,
        OperationKind::Query | OperationKind::Retrieve => match mode {
            JobExecutionMode::Foreground => JobPolicy::Synchronous,
            JobExecutionMode::Detached
            | JobExecutionMode::LongRunningProvider
            | JobExecutionMode::ArtifactBacked => JobPolicy::JobBacked,
        },
    }
}
```

Export it from `mod.rs`:

```rust
pub mod job_policy;
pub use job_policy::*;
```

- [ ] **Step 4: Run the test**

Run:

```bash
cargo test -p axon-api source_job_policy --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-api/src/source/job_policy.rs crates/axon-api/src/source/mod.rs crates/axon-api/src/source_job_policy_tests.rs
git commit -m "feat(api): define job backed operation policy"
```

## Task 2: Complete Job DTOs And State Machine

**Files:**

- Modify: `crates/axon-api/src/source/job.rs`
- Modify: `crates/axon-jobs/src/state_machine.rs`
- Test: `crates/axon-api/src/source_job_dto_tests.rs`
- Test: `crates/axon-jobs/src/state_machine_tests.rs`

**Interfaces:**

- Consumes: `LifecycleStatus`, `ApiError`, and job contract status table.
- Produces: full job DTOs and transition validator.

- [ ] **Step 1: Add state-machine table tests**

Add `crates/axon-jobs/src/state_machine_tests.rs`:

```rust
use axon_api::source::{JobId, LifecycleStatus};
use uuid::Uuid;

use crate::state_machine::validate_transition;

#[test]
fn job_state_machine_accepts_only_contract_transitions() {
    let job_id = JobId::new(Uuid::from_u128(1));
    let allowed = [
        (LifecycleStatus::Queued, LifecycleStatus::Blocked),
        (LifecycleStatus::Queued, LifecycleStatus::Running),
        (LifecycleStatus::Queued, LifecycleStatus::Canceling),
        (LifecycleStatus::Queued, LifecycleStatus::Expired),
        (LifecycleStatus::Pending, LifecycleStatus::Queued),
        (LifecycleStatus::Pending, LifecycleStatus::Running),
        (LifecycleStatus::Pending, LifecycleStatus::Canceling),
        (LifecycleStatus::Pending, LifecycleStatus::Expired),
        (LifecycleStatus::Blocked, LifecycleStatus::Queued),
        (LifecycleStatus::Blocked, LifecycleStatus::Running),
        (LifecycleStatus::Blocked, LifecycleStatus::Canceling),
        (LifecycleStatus::Blocked, LifecycleStatus::Failed),
        (LifecycleStatus::Blocked, LifecycleStatus::Expired),
        (LifecycleStatus::Running, LifecycleStatus::Waiting),
        (LifecycleStatus::Running, LifecycleStatus::Canceling),
        (LifecycleStatus::Running, LifecycleStatus::Completed),
        (LifecycleStatus::Running, LifecycleStatus::CompletedDegraded),
        (LifecycleStatus::Running, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Running),
        (LifecycleStatus::Waiting, LifecycleStatus::Canceling),
        (LifecycleStatus::Waiting, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Expired),
        (LifecycleStatus::Canceling, LifecycleStatus::Canceled),
        (LifecycleStatus::Canceling, LifecycleStatus::Failed),
    ];
    for (from, to) in allowed {
        validate_transition(job_id, from, to).expect("allowed transition");
    }

    for terminal in [
        LifecycleStatus::Completed,
        LifecycleStatus::CompletedDegraded,
        LifecycleStatus::Failed,
        LifecycleStatus::Canceled,
        LifecycleStatus::Expired,
        LifecycleStatus::Skipped,
    ] {
        let err = validate_transition(job_id, terminal, LifecycleStatus::Queued)
            .expect_err("terminal transition rejected");
        assert_eq!(err.code, "job.invalid_transition");
    }
}
```

- [ ] **Step 2: Add required DTO field round-trip test**

In `crates/axon-api/src/source_job_dto_tests.rs`, add a serde round trip asserting `JobCreateRequest` contains:

```rust
assert!(json.get("auth_snapshot").is_some());
assert!(json.get("config_snapshot_id").is_some());
assert!(json.get("stage_plan").is_some());
assert!(json.get("requirements").is_some());
assert!(json.get("result_schema").is_some());
assert!(json.get("parent_job_id").is_some());
assert!(json.get("root_job_id").is_some());
assert!(json.get("attempt").is_some());
assert!(json.get("warnings").is_some());
assert!(json.get("error").is_some());
```

- [ ] **Step 3: Run tests and confirm failures**

Run:

```bash
cargo test -p axon-api source_job_dto --no-fail-fast
cargo test -p axon-jobs state_machine --no-fail-fast
```

Expected: FAIL if DTO fields or test module are incomplete.

- [ ] **Step 4: Implement missing DTO fields and state exports**

Extend job DTOs in `axon-api` with the exact contract field names. Keep serde names snake_case and use `deny_unknown_fields` on request DTOs. Ensure `LifecycleStatus::Skipped` is terminal in any helper that checks terminal status.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p axon-api source_job_dto --no-fail-fast
cargo test -p axon-jobs state_machine --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-api/src/source crates/axon-api/src/source_job_dto_tests.rs crates/axon-jobs/src/state_machine.rs crates/axon-jobs/src/state_machine_tests.rs
git commit -m "feat(jobs): complete job dto and state machine contract"
```

## Task 3: Add Unified SQLite Tables, Indexes, And Cursor Pages

**Files:**

- Create: `crates/axon-jobs/src/unified/schema.rs`
- Create: `crates/axon-jobs/src/unified/pagination.rs`
- Modify: `crates/axon-jobs/src/migrations.rs`
- Add migration under: `crates/axon-jobs/src/migrations/`
- Test: `crates/axon-jobs/src/unified_tests.rs`
- Test: `crates/axon-jobs/src/migrations_tests.rs`

**Interfaces:**

- Consumes: `JobCreateRequest`, `JobListRequest`, `JobEventListRequest`.
- Produces: unified tables and O(page size) list/event/status query plans.

- [ ] **Step 1: Add migration/index tests**

In `unified_tests.rs`, add:

```rust
#[tokio::test]
async fn unified_job_tables_have_contract_indexes() {
    let pool = crate::store::open_sqlite_pool(":memory:").await.unwrap();
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_axon_jobs_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    for required in [
        "idx_axon_jobs_status_kind_updated",
        "idx_axon_jobs_source_status_updated",
        "idx_axon_jobs_watch_status_updated",
        "idx_axon_job_events_job_sequence",
        "idx_axon_job_events_job_severity_sequence",
        "idx_axon_job_attempts_job_attempt",
        "idx_axon_job_stages_job_stage",
    ] {
        assert!(indexes.iter().any(|name| name == required), "missing {required}");
    }
}
```

- [ ] **Step 2: Add cursor pagination tests**

Add:

```rust
#[tokio::test]
async fn job_events_page_after_sequence_reads_only_next_page() {
    let pool = crate::store::open_sqlite_pool(":memory:").await.unwrap();
    let store = crate::unified::SqliteUnifiedJobStore::new(pool);
    let job = store.create(job_request_fixture("source")).await.unwrap();
    append_events(&store, job.job_id, 25).await;

    let page = store
        .events(JobEventListRequest {
            job_id: job.job_id,
            after_sequence: Some(10),
            limit: Some(5),
            severity: None,
            visibility: None,
            cursor: None,
        })
        .await
        .unwrap();

    assert_eq!(page.events.len(), 5);
    assert_eq!(page.events[0].sequence, 11);
    assert_eq!(page.events[4].sequence, 15);
    assert!(page.next_cursor.is_some());
}
```

- [ ] **Step 3: Run tests and confirm failure**

Run:

```bash
cargo test -p axon-jobs unified_job_tables_have_contract_indexes job_events_page_after_sequence_reads_only_next_page --no-fail-fast
```

Expected: FAIL until tables/indexes/cursors are complete.

- [ ] **Step 4: Add unified tables**

Add migration SQL for:

```sql
CREATE TABLE axon_jobs (
  job_id TEXT PRIMARY KEY,
  job_kind TEXT NOT NULL,
  job_intent TEXT NOT NULL,
  status TEXT NOT NULL,
  phase TEXT NOT NULL,
  request_id TEXT NOT NULL,
  source_id TEXT,
  watch_id TEXT,
  parent_job_id TEXT,
  root_job_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  priority TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  started_at INTEGER,
  updated_at INTEGER NOT NULL,
  deadline_at INTEGER,
  completed_at INTEGER,
  idempotency_key TEXT,
  auth_snapshot_json TEXT NOT NULL,
  config_snapshot_id TEXT NOT NULL,
  stage_plan_json TEXT NOT NULL,
  requirements_json TEXT NOT NULL,
  result_schema TEXT NOT NULL,
  warnings_json TEXT NOT NULL DEFAULT '[]',
  error_json TEXT,
  request_json TEXT NOT NULL,
  result_json TEXT
);

CREATE TABLE axon_job_attempts (
  job_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  status TEXT NOT NULL,
  worker_id TEXT,
  started_at INTEGER,
  completed_at INTEGER,
  error_json TEXT,
  PRIMARY KEY (job_id, attempt)
);

CREATE TABLE axon_job_stages (
  job_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  stage_id TEXT NOT NULL,
  phase TEXT NOT NULL,
  status TEXT NOT NULL,
  required INTEGER NOT NULL,
  provider_requirements_json TEXT NOT NULL,
  input_counts_json TEXT NOT NULL DEFAULT '{}',
  output_counts_json TEXT NOT NULL DEFAULT '{}',
  started_at INTEGER,
  completed_at INTEGER,
  error_json TEXT,
  PRIMARY KEY (job_id, attempt, stage_id)
);

CREATE TABLE axon_job_events (
  job_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  event_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  stage_id TEXT,
  batch_id TEXT,
  reservation_id TEXT,
  checkpoint_id TEXT,
  dedupe_key TEXT,
  severity TEXT,
  visibility TEXT,
  event_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (job_id, sequence)
);

CREATE TABLE axon_job_heartbeats (
  job_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  worker_id TEXT NOT NULL,
  phase TEXT NOT NULL,
  stage_id TEXT,
  last_event_sequence INTEGER NOT NULL,
  progress_counts_json TEXT NOT NULL DEFAULT '{}',
  provider_reservations_json TEXT NOT NULL DEFAULT '[]',
  heartbeat_at INTEGER NOT NULL,
  PRIMARY KEY (job_id, attempt)
);

CREATE TABLE axon_job_migration_watermarks (
  source_table TEXT PRIMARY KEY,
  migrated_at INTEGER NOT NULL,
  row_count INTEGER NOT NULL
);
```

Add indexes named in Step 1.

- [ ] **Step 5: Implement cursor helpers**

In `pagination.rs`, implement opaque base64url JSON cursors:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobCursor {
    pub updated_at: i64,
    pub job_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct EventCursor {
    pub sequence: u64,
}
```

Expose `encode_job_cursor`, `decode_job_cursor`, `encode_event_cursor`, `decode_event_cursor`.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p axon-jobs unified --no-fail-fast
cargo test -p axon-jobs migrations --no-fail-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-jobs/src/unified/schema.rs crates/axon-jobs/src/unified/pagination.rs crates/axon-jobs/src/unified.rs crates/axon-jobs/src/migrations.rs crates/axon-jobs/src/migrations crates/axon-jobs/src/unified_tests.rs crates/axon-jobs/src/migrations_tests.rs
git commit -m "feat(jobs): add unified durable job tables"
```

## Task 4: Make Unified JobStore Authoritative

**Files:**

- Modify: `crates/axon-jobs/src/unified/{control.rs,ops.rs,heartbeat.rs,observe.rs}`
- Modify: `crates/axon-jobs/src/boundary.rs`
- Test: `crates/axon-jobs/src/unified_tests.rs`

**Interfaces:**

- Consumes: `JobStore` trait.
- Produces: full create/get/list/events/update/cancel/retry/recover/cleanup/artifacts/reset semantics backed by unified tables.

- [ ] **Step 1: Add invalid transition mutation test**

Add:

```rust
#[tokio::test]
async fn invalid_transition_fails_without_mutating_job() {
    let store = unified_store().await;
    let job = store.create(job_request_fixture("source")).await.unwrap();
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Fetching,
            warnings: vec![],
            error: None,
        })
        .await
        .unwrap();

    let err = store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Queued,
            phase: PipelinePhase::Resolving,
            warnings: vec![],
            error: None,
        })
        .await
        .expect_err("running -> queued invalid");
    assert_eq!(err.code, "job.invalid_transition");
    assert_eq!(
        store.get(job.job_id).await.unwrap().unwrap().status,
        LifecycleStatus::Running
    );
}
```

- [ ] **Step 2: Add monotonic event sequence test**

Add:

```rust
#[tokio::test]
async fn append_events_assigns_monotonic_per_job_sequence() {
    let store = unified_store().await;
    let job = store.create(job_request_fixture("source")).await.unwrap();

    for idx in 0..3 {
        store.append_event(progress_event_fixture(job.job_id, idx)).await.unwrap();
    }

    let page = store
        .events(JobEventListRequest {
            job_id: job.job_id,
            after_sequence: None,
            limit: Some(10),
            severity: None,
            visibility: None,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(page.events.iter().map(|event| event.sequence).collect::<Vec<_>>(), vec![1, 2, 3]);
}
```

- [ ] **Step 3: Run tests and confirm failures**

Run:

```bash
cargo test -p axon-jobs invalid_transition_fails_without_mutating_job append_events_assigns_monotonic_per_job_sequence --no-fail-fast
```

Expected: FAIL until unified store writes status/events atomically.

- [ ] **Step 4: Implement authoritative store methods**

Implement all `JobStore` methods against unified tables:

- `create`: insert `axon_jobs`, attempt `1`, planned stages, and first event sequence `1`.
- `update_status`: transactionally check current status, call `validate_transition`, update current row, update active attempt/stage, append status event.
- `append_event`: transactionally increment `last_event_sequence` on the job or attempt row and use the returned value as the event sequence.
- `heartbeat`: upsert `axon_job_heartbeats` and update `axon_jobs.updated_at`.
- `list`: keyset page by `(updated_at, job_id)` and filters.
- `events`: page by `(job_id, sequence)` and optional severity/visibility.
- `cancel`: `queued -> canceled` or `running/waiting/blocked -> canceling`.
- `retry`: append a new attempt under the same `job_id`, increment `attempt`, restore status to `queued` or `blocked`.
- `recover`: inspect heartbeat and lease grace; append new attempts only when safe.
- `cleanup`: delete terminal rows/events by retention policy.
- `artifacts`: page job artifacts by `job_id`.
- `reset`: clear unified job tables in FK-safe order.

- [ ] **Step 5: Run unified store tests**

Run:

```bash
cargo test -p axon-jobs unified --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-jobs/src/unified crates/axon-jobs/src/boundary.rs crates/axon-jobs/src/unified_tests.rs
git commit -m "feat(jobs): make unified job store authoritative"
```

## Task 5: Block Incompatible Legacy Job Stores

**Files:**

- Modify: `crates/axon-jobs/src/unified.rs`
- Modify/create: `crates/axon-jobs/src/store_inventory.rs`
- Test: `crates/axon-jobs/src/unified_tests.rs`

**Interfaces:**

- Consumes: legacy family tables `axon_crawl_jobs`, `axon_embed_jobs`, `axon_extract_jobs`, `axon_ingest_jobs`, existing watch history.
- Produces: incompatible-store blockers that prevent unified workers from running before reset/preflight receipts prove a clean break.

- [ ] **Step 1: Add incompatible-store blocker tests**

Add:

```rust
#[tokio::test]
async fn non_empty_legacy_family_rows_block_unified_workers() {
    let pool = crate::store::open_sqlite_pool(":memory:").await.unwrap();
    insert_legacy_crawl_job(&pool, "running").await;

    let blocker = crate::unified::detect_incompatible_legacy_jobs(&pool)
        .await
        .unwrap()
        .expect("non-empty legacy table blocks cutover");
    assert!(blocker.legacy_tables.contains(&"axon_crawl_jobs".to_string()));
    assert!(blocker.message.contains("axon reset"));
}
```

Add reset receipt guard:

```rust
#[tokio::test]
async fn reset_receipt_allows_fresh_unified_schema_without_importing_legacy_rows() {
    let pool = crate::store::open_sqlite_pool(":memory:").await.unwrap();
    insert_legacy_extract_job(&pool, "canceled").await;
    write_reset_receipt(&pool, "legacy jobs cleared").await;

    assert!(crate::unified::detect_incompatible_legacy_jobs(&pool).await.unwrap().is_none());
}
```

- [ ] **Step 2: Run blocker tests and confirm failure**

Run:

```bash
cargo test -p axon-jobs non_empty_legacy_family_rows_block_unified_workers reset_receipt_allows_fresh_unified_schema_without_importing_legacy_rows --no-fail-fast
```

Expected: FAIL until blockers exist.

- [ ] **Step 3: Implement incompatible-store blocker**

Implement blocker rules:

- Inspect known legacy job tables with bounded count queries.
- If any legacy table is non-empty and no reset/clear receipt is present, block unified workers before side effects.
- Return an actionable error naming the table, row count estimate, data dir, and reset/preflight command.
- Do not import, backfill, preserve, or execute legacy job rows.

- [ ] **Step 4: Run blocker tests**

Run:

```bash
cargo test -p axon-jobs legacy_family_rows_block_unified_workers unified --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-jobs/src/unified.rs crates/axon-jobs/src/store_inventory.rs crates/axon-jobs/src/unified_tests.rs
git commit -m "feat(jobs): block incompatible legacy job stores"
```

## Task 6: Cut Workers And Services Over To Unified Jobs

**Files:**

- Modify: `crates/axon-jobs/src/workers.rs`
- Modify: `crates/axon-jobs/src/workers/**`
- Modify: `crates/axon-services/src/runtime.rs`
- Modify: `crates/axon-services/src/jobs.rs`
- Modify: source/watch/extract/research/memory/graph/prune/provider/reset services
- Test: `crates/axon-services/src/jobs_tests.rs`
- Test: `crates/axon-jobs/src/workers_tests.rs`

**Interfaces:**

- Consumes: `JobStore` unified jobs.
- Produces: all job-backed operations enqueue and run from unified jobs only.

- [ ] **Step 1: Add full operation routing tests**

In `crates/axon-services/src/jobs_tests.rs`, add:

```rust
#[tokio::test]
async fn every_job_backed_operation_creates_unified_job_descriptor() {
    let ctx = test_context_with_unified_jobs().await;

    // `Reset` is intentionally excluded — see "Scope Exception: `reset` Stays
    // Jobless" above. `reset`'s dry-run default must not create/migrate the
    // SQLite DB, which every job-backed tracking path does as a side effect,
    // so it is not wired through enqueue_operation despite being classified
    // JobBacked in job_policy_for_operation for DTO/schema completeness.
    for operation in [
        OperationKind::Source,
        OperationKind::Watch,
        OperationKind::Extract,
        OperationKind::Research,
        OperationKind::MemoryCompaction,
        OperationKind::MemoryImport,
        OperationKind::GraphMutation,
        OperationKind::Prune,
        OperationKind::ProviderProbe,
    ] {
        let descriptor = enqueue_operation_fixture(&ctx, operation).await.unwrap();
        assert_eq!(descriptor.status, LifecycleStatus::Queued);
        assert!(descriptor.poll_after_ms > 0);
        assert_eq!(descriptor.poll_request.action, "jobs");
        assert_eq!(descriptor.poll_request.subaction.as_deref(), Some("get"));
    }
}

#[tokio::test]
async fn normal_query_and_retrieve_do_not_create_jobs() {
    let ctx = test_context_with_unified_jobs().await;
    query_fixture(&ctx).await.unwrap();
    retrieve_fixture(&ctx).await.unwrap();

    let jobs = ctx.job_store().list(JobListRequest::default()).await.unwrap();
    assert!(jobs.items.is_empty());
}
```

- [ ] **Step 2: Add stale recovery guard**

In `crates/axon-jobs/src/workers_tests.rs`, add:

```rust
#[tokio::test]
async fn stale_recovery_does_not_double_publish_when_original_attempt_is_alive() {
    let harness = provider_heavy_job_harness().await;
    let job = harness.enqueue_source_embedding_job().await;
    harness.mark_attempt_running_with_fresh_provider_reservation(job.job_id).await;

    let recovered = harness.store.recover(JobRecoveryRequest {
        kind: Some(JobKind::Source),
        stale_before: Some(harness.now_minus_grace()),
        limit: Some(10),
    }).await.unwrap();

    assert!(recovered.recovered_job_ids.is_empty());
    assert_eq!(harness.publish_count(job.job_id).await, 0);
}
```

- [ ] **Step 3: Run routing/recovery tests and confirm failure**

Run:

```bash
cargo test -p axon-services every_job_backed_operation_creates_unified_job_descriptor normal_query_and_retrieve_do_not_create_jobs --no-fail-fast
cargo test -p axon-jobs stale_recovery_does_not_double_publish_when_original_attempt_is_alive --no-fail-fast
```

Expected: FAIL until services and workers use unified jobs.

- [ ] **Step 4: Route all job-backed service operations**

Update `ServiceContext` so long-running services receive `Arc<dyn JobStore>`. Replace family `JobKind` usage in `crates/axon-services/src/jobs.rs` with unified `JobListRequest`, `JobEventListRequest`, `JobCancelRequest`, `JobRetryRequest`, and `JobRecoveryRequest`.

For each job-backed service path:

- Build `JobCreateRequest` with auth snapshot, config snapshot, stage plan, requirements, result schema, root/parent ids, warnings, and request JSON.
- Return `JobDescriptor` for detached calls.
- For `--wait true`/foreground source work, create the job row and execute through the same runner.
- Keep normal query/retrieve jobless unless they enter long-running provider or artifact-backed mode.

- [ ] **Step 5: Cut workers to unified polling**

Workers poll `axon_jobs` by `status IN ('queued','waiting','blocked')`, priority, and provider reservations. Stage runners read `job_kind` + request JSON and dispatch to source/watch/extract/research/memory/graph/prune/provider/reset runners.

Preserve:

- panic guard writes failed attempt + event
- cancellation token checks between batches
- heartbeat before/after provider reservations
- recovery creates new attempt under same `job_id`
- retry uses immutable request/config snapshot

- [ ] **Step 6: Run services/workers tests**

Run:

```bash
cargo test -p axon-services jobs --no-fail-fast
cargo test -p axon-jobs workers unified --no-fail-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-jobs/src/workers.rs crates/axon-jobs/src/workers crates/axon-services/src
git commit -m "feat(jobs): route services and workers through unified jobs"
```

## Task 7: Expose Unified Jobs Through CLI, MCP, REST/SSE, And Web

**Files:**

- Modify: `crates/axon-cli/src/**`
- Modify: `crates/axon-mcp/src/**`
- Modify: `crates/axon-web/src/**`
- Test: CLI/MCP/web job tests in each crate

**Interfaces:**

- Consumes: unified jobs service.
- Produces: jobs list/get/events/stream/cancel/retry/recover/cleanup/artifacts across every public surface.

- [ ] **Step 1: Add surface parity tests**

Add tests asserting:

```rust
assert_cli_has_jobs_subcommands(["list", "get", "events", "stream", "cancel", "retry", "recover", "cleanup", "clear"]);
assert_mcp_jobs_action_has_subactions(["list", "get", "events", "cancel", "retry", "recover", "cleanup", "clear"]);
assert_rest_route_exists("GET", "/v1/jobs");
assert_rest_route_exists("GET", "/v1/jobs/{job_id}");
assert_rest_route_exists("GET", "/v1/jobs/{job_id}/events");
assert_rest_route_exists("GET", "/v1/jobs/{job_id}/stream");
assert_rest_route_exists("POST", "/v1/jobs/{job_id}/cancel");
assert_rest_route_exists("POST", "/v1/jobs/{job_id}/retry");
assert_rest_route_exists("POST", "/v1/jobs/recover");
assert_rest_route_exists("POST", "/v1/jobs/cleanup");
```

- [ ] **Step 2: Add event cursor parity test**

Add one service-backed test per surface that creates three events and requests `after_sequence=1`; each must return event sequences `[2, 3]` and a stable next cursor when limited.

- [ ] **Step 3: Run surface tests and confirm failure**

Run:

```bash
cargo test -p axon-cli jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
```

Expected: FAIL until surfaces route through unified jobs.

- [ ] **Step 4: Implement CLI jobs commands**

Commands:

```text
axon jobs list --status <status> --kind <kind> --limit <n> --cursor <cursor>
axon jobs get <job_id> --include events,artifacts
axon jobs events <job_id> --after-sequence <n> --limit <n>
axon jobs stream <job_id> --after-sequence <n>
axon jobs cancel <job_id> --reason <text>
axon jobs retry <job_id> --mode same-config|with-overrides
axon jobs recover --kind <kind> --stale-before <duration> --limit <n>
axon jobs cleanup --older-than <duration> --status <status> --dry-run
axon jobs clear --status <status> --older-than <duration> --confirm
```

Render progress from event pages and cached job rows with the same phase/status/warnings/error fields.

- [ ] **Step 5: Implement MCP jobs action**

Route `action=jobs` subactions from `tool-contract.md` to unified service:

```text
list, get, events, cancel, retry, recover, cleanup, clear
```

`JobDescriptor` responses include `job_id`, `kind`, `status`, `phase`, `poll_after_ms`, and the exact MCP polling request.

- [ ] **Step 6: Implement REST/SSE routes**

Add routes from `rest-contract.md`:

```text
GET /v1/jobs
GET /v1/jobs/{job_id}
GET /v1/jobs/{job_id}/events
GET /v1/jobs/{job_id}/stream
GET /v1/jobs/{job_id}/artifacts
POST /v1/jobs/recover
POST /v1/jobs/cleanup
DELETE /v1/jobs
POST /v1/jobs/{job_id}/cancel
POST /v1/jobs/{job_id}/retry
```

SSE stream resumes from `after_sequence` and emits heartbeat frames at `heartbeat_ms`.

- [ ] **Step 7: Run surface tests**

Run:

```bash
cargo test -p axon-cli jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-cli/src crates/axon-mcp/src crates/axon-web/src
git commit -m "feat(jobs): expose unified jobs on all surfaces"
```

## Task 8: Remove Legacy Family Readers And Tables From Active Runtime

**Files:**

- Modify: `crates/axon-jobs/src/backend.rs`
- Modify: `crates/axon-jobs/src/query.rs`
- Modify: `crates/axon-jobs/src/ops.rs`
- Modify: `crates/axon-services/src/jobs.rs`
- Modify: docs/reference generated artifacts if schema generator updates them
- Test: `crates/axon-jobs/src/unified_tests.rs`

**Interfaces:**

- Consumes: migration parity and unified surface tests from earlier tasks.
- Produces: no active runtime reader/writer for `axon_crawl_jobs`, `axon_embed_jobs`, `axon_extract_jobs`, or `axon_ingest_jobs`.

- [ ] **Step 1: Add legacy access blocker test**

Add:

```rust
#[test]
fn active_runtime_no_longer_names_legacy_family_tables() {
    let source = include_str!("backend.rs").to_string()
        + include_str!("query.rs")
        + include_str!("ops.rs");
    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        assert!(!source.contains(table), "legacy table {table} still referenced in active runtime");
    }
}
```

Keep migration modules exempt from this test.

- [ ] **Step 2: Run blocker and confirm failure**

Run:

```bash
cargo test -p axon-jobs active_runtime_no_longer_names_legacy_family_tables --no-fail-fast
```

Expected: FAIL until legacy active readers/writers are removed.

- [ ] **Step 3: Remove active family API**

Remove or quarantine:

- `JobKind::{Crawl, Embed, Extract, Ingest}` table mapping for active runtime.
- `JobPayload` variants as queue selectors.
- family `list_jobs(kind)`, `job_status(kind)`, `cancel_job(kind)`, `cleanup_jobs(kind)`, `clear_jobs(kind)` service APIs.

Keep a migration-only module for old rows until the next major cleanup.

- [ ] **Step 4: Run full job crate tests**

Run:

```bash
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services jobs --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-jobs/src crates/axon-services/src/jobs.rs
git commit -m "refactor(jobs): remove legacy family job runtime"
```

## Evidence Note (2026-07-09): Tasks 6 and 8 partially closed by the follow-up plan

`docs/pipeline-unification/plans/2026-07-08-finish-job-cutover-and-security-completion.md`
(branch `finish-job-cutover-impl`) closed the **Crawl/Embed/Ingest/Extract**
slice of Task 6 and Task 8's scope — not the full Task 6 operation set
(`Source`/`Watch`/`Research`/`Memory`/`Graph`/`Prune`/`ProviderProbe`), which
remains open. Do not check the boxes above as fully done; this note exists so
a future reader does not have to re-derive what happened from commit history.

**Task 6 (this plan) → done for Crawl/Embed/Ingest, Extract already done
before this plan:**
- Extract was already cut over to the unified `JobStore` before the
  2026-07-08 plan started (`ExtractRunner`/`extract_bridge.rs` — the proven
  template the follow-up plan copied).
- Embed: `97124ca14` — `EmbedRunner` + `embed_bridge.rs`.
- Crawl: `46575ef6a` — `CrawlRunner` + `crawl_bridge.rs` (the disputed `!Send`
  claim was empirically disproven; the plain runner shape was used, not the
  thread-isolation fallback).
- Ingest: `4c2effea4` — `IngestRunner` + `ingest_bridge.rs` (only
  `IngestSource::Sessions` executes; every other variant already returns a
  clean "no longer supported" error post-Phase-12).
- `Source`/`Watch`/`Research`/`Memory`/`Graph`/`Prune`/`ProviderProbe` are
  **not** part of this closure — those operations' job-backed cutover (if
  not already unified elsewhere) remains open work outside the 2026-07-08
  plan's scope.

**Task 8 (this plan) → partially done, with two live blockers documented
instead of silently worked around:**
`ca7ea71d1` retired the legacy in-process worker lanes
(`crawl_worker`/`embed_worker`/`ingest_worker` and their now-orphaned support
modules `heartbeat.rs`/`panic_guard.rs`/`progress.rs`/`workers/runners/*`) —
no in-process execution runs against `axon_crawl_jobs`/`axon_embed_jobs`/
`axon_ingest_jobs` anymore. However, `crates/axon-jobs/src/backend.rs` and
`crates/axon-jobs/src/query.rs`'s table mappings/query functions for
`JobKind::{Crawl,Embed,Ingest}` were **deliberately kept**, because an audit
found two still-live call sites this plan's Task 8 did not account for:
- `crates/axon-jobs/src/watch/dispatch.rs::enqueue_change_crawl`/
  `crawl_job_active` still write/read `axon_crawl_jobs` directly for watch's
  clustered re-crawl dispatch (`axon watch exec`, `POST /v1/watch/{id}/run`,
  the automatic watch scheduler).
- `crates/axon-services/src/refresh.rs`'s `latest_crawl_config_json`/
  `latest_ingest_config_json` still read the legacy tables for `axon refresh`'s
  config-snapshot replay.

Porting those two call sites to the unified store is required before Task
8's `rg` verification in Task 9 Step 4 can show zero non-migration/bridge
matches for Crawl/Embed/Ingest. Tracked as follow-up work, not done here.
`SqliteServiceRuntime::count_jobs`/`count_jobs_by_status` were bridged for
all four kinds (`a500ae416`'s parent commit `ca7ea71d1`) so `axon status`,
the queue-summary logger, and the starvation watchdog read accurate counts
in the interim.

## Task 9: Full Cutover Verification

**Files:**

- Modify if needed: `docs/pipeline-unification/plans/2026-07-04-full-durable-job-cutover.md`
- Modify after implementation: GitHub issue #298 checklist and verification comment.

**Interfaces:**

- Consumes: all prior task commits.
- Produces: full cutover evidence.

- [ ] **Step 1: Run contract tests**

Run:

```bash
cargo test -p axon-api source_job --no-fail-fast
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services jobs --no-fail-fast
```

Expected: PASS.

- [ ] **Step 2: Run surface tests**

Run:

```bash
cargo test -p axon-cli jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
```

Expected: PASS.

- [ ] **Step 3: Run end-to-end job-backed operation smoke tests**

Run:

```bash
cargo test -p axon-services source_job_backed --no-fail-fast
cargo test -p axon-services watch_job_backed --no-fail-fast
cargo test -p axon-services prune_job_backed --no-fail-fast
cargo test -p axon-services reset_job_backed --no-fail-fast
```

Expected: PASS. If exact filters differ after implementation, run the corresponding source/watch/prune/reset job-backed suites and record exact commands in the issue comment.

- [ ] **Step 4: Verify legacy runtime removal**

Run:

```bash
rg -n "axon_crawl_jobs|axon_embed_jobs|axon_extract_jobs|axon_ingest_jobs|JobKind::Crawl|JobKind::Embed|JobKind::Extract|JobKind::Ingest" crates/axon-jobs crates/axon-services crates/axon-cli crates/axon-mcp crates/axon-web
```

Expected: matches only in migration/import tests or historical docs, not active runtime.

- [ ] **Step 5: Verify event pagination and recovery guards**

Run:

```bash
cargo test -p axon-jobs after_sequence --no-fail-fast
cargo test -p axon-jobs stale_recovery --no-fail-fast
cargo test -p axon-jobs cancellation --no-fail-fast
cargo test -p axon-jobs retry --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Update issue #298**

Use:

```bash
gh issue view 298 --json body > /tmp/issue-298.json
```

Check off only the durable-job cutover items proven by the commands above and add a comment with exact command results and remaining follow-ups for logs/traces parity if any.

- [ ] **Step 7: Commit plan/evidence updates**

```bash
git add docs/pipeline-unification/plans/2026-07-04-full-durable-job-cutover.md
git commit -m "docs(pipeline): plan full durable job cutover"
```

## Self-Review

- Spec coverage: job-backed/synchronous classification is Task 1; full status/state machine and required fields are Task 2; unified SQLite tables/indexes/pagination are Task 3; authoritative store/events/heartbeats/cancel/retry/recover/cleanup are Task 4; id-preserving migration is Task 5; workers/services full cutover is Task 6; CLI/MCP/REST/SSE exposure is Task 7; legacy reader removal is Task 8; full verification is Task 9.
- Placeholder scan: this plan contains no deferred placeholder language.
- Type consistency: the plan consistently uses `JobStore`, `JobCreateRequest`, `JobDescriptor`, `JobListRequest`, `JobEventListRequest`, `JobStatusUpdate`, `JobRecoveryRequest`, `JobCleanupRequest`, `LifecycleStatus`, `PipelinePhase`, `JobId`, and `ApiError`.
