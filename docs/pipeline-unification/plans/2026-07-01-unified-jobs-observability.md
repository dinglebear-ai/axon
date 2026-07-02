# PR10 Plan: Unified Jobs And Observability

> **Status:** Active planning for PR10. Completed task checkboxes mark
> implemented work; final merge-gate items remain unchecked until the
> pre-merge audit, required checks, mandatory reviews, and merge actually
> complete.
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

Issue: [#298](https://github.com/jmagar/axon/issues/298)
Branch: `codex/unified-jobs-observability`
Base: `main` after PR9 vector/embedding split

## Goal

Implement the planned PR10 slice from issue #298:

> **Unified jobs/observability** — convert toward one source job model with
> `job_id`, stages, events, heartbeats, provider reservations, cooling,
> progress output, recovery, cancellation, and starvation protection.

This PR establishes the target job and observability runtime foundation. It
must not cut over public CLI/MCP/REST source surfaces, delete existing
family-specific job commands, or port source families. Current runtime behavior
must continue to work while the target `JobStore`/`ObservabilitySink` model
becomes real enough for later source-family PRs to use.

## Architecture

Current runtime job behavior is still family-oriented in `axon-jobs`: crawl,
embed, extract, ingest, sessions, freshness, and watch paths have their own
payloads and status projections. PR10 introduces the durable target model beside
that runtime, then bridges safe shared primitives into the current code where
doing so does not change public behavior.

The target model is:

```text
Job
  -> JobAttempt
  -> JobStage
  -> SourceProgressEvent
  -> JobHeartbeat
  -> JobArtifact
  -> JobResult / SourceJobStatus
```

`axon-api` owns data shapes and external enum projections. `axon-observe` owns
event builders, heartbeat builders, metric/log helper types, provider wait
visibility, and test sinks. `axon-jobs` owns persistence, state transitions,
event sequencing, recovery/cancellation/retry semantics, and the fake/SQLite
`JobStore` implementations. Domain crates emit progress through
`axon-observe`; they do not update SQLite job rows directly in new target code.

Provider reservations remain in `axon-observe` for now because the reservation
manager already landed there in PR4. PR10 makes reservation/wait/cooling state
observable and joinable to `job_id`, `stage_id`, and heartbeat snapshots. The
embedding/vector/LLM providers must not become global schedulers.

## Tech Stack

Rust 2024, `async_trait`, `chrono`, `serde`, `serde_json`, `schemars`,
`utoipa`, `sqlx` SQLite migrations, `axon-api::source` DTOs, `axon-error`
typed errors, `axon-observe` event/heartbeat/reservation helpers,
`axon-jobs` `JobStore` and in-process runtime.

## Global Constraints

- Use TDD: every production behavior change starts with a failing sibling test.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Keep production Rust modules under 500 LOC; split modules before they become
  dumping grounds.
- Use sibling `*_tests.rs` files; do not add inline `#[cfg(test)] mod tests`.
- Do not wire public CLI/MCP/REST surface cutover in this PR.
- Do not remove existing `crawl`, `embed`, `extract`, `ingest`, `sessions`,
  `watch`, or job family surfaces in this PR.
- Do not port local/web/git/feed/session source families in this PR.
- Do not require Qdrant, TEI, LLMs, Chrome, network, or real credentials for
  PR10 tests.
- Do not introduce compatibility aliases for future removed surfaces.
- Every new durable target job row is keyed by `job_id`; `run_id` is not a
  correlation identifier.
- Every new target job event has a monotonic per-job sequence.
- Active target jobs must be monitorable from heartbeats without log scraping.
- Failed/degraded target events carry structured `ApiError` or warning payloads.
- Provider wait/cooling must surface as observable `waiting` state, not silent
  stalls.
- Current legacy family queues may continue to exist until source-family and
  public surface cutover PRs consume the target job model.
- Commit early after each task's verification passes.

## Current-State Anchors

- Target API DTOs: `crates/axon-api/src/source/{enums,ids,lifecycle,status,stage,state,listing,common}.rs`.
- Current target fake store boundary: `crates/axon-jobs/src/boundary.rs`.
- Current legacy family job runtime: `crates/axon-jobs/src/{backend,ops,query,runtime,workers}.rs`.
- Current legacy progress persistence: `progress_json` and worker progress
  helpers under `crates/axon-jobs/src/workers/progress.rs`.
- Current heartbeat behavior: `crates/axon-jobs/src/workers/heartbeat.rs`
  touches legacy `updated_at`.
- Current starvation detector: `crates/axon-jobs/src/workers/starvation.rs`.
- Current provider reservations: `crates/axon-observe/src/reservation.rs`.
- Current `axon-observe` skeleton modules:
  `event.rs`, `heartbeat.rs`, `metric.rs`, `progress.rs`, `span.rs`,
  `testing.rs`, and `collector.rs`.
- Target contracts:
  - `docs/pipeline-unification/runtime/job-contract.md`
  - `docs/pipeline-unification/runtime/observability-contract.md`
  - `docs/pipeline-unification/runtime/provider-contract.md`
  - `docs/pipeline-unification/schemas/event-schema.md`
  - `docs/pipeline-unification/schemas/database-schema.md`

## Non-Goals

- No public command/help/schema removal.
- No REST route cutover to `/v1/jobs` as the only public route.
- No MCP tool cutover.
- No web/Android/Palette/Chrome-extension client migration.
- No live source-family port onto the target job runtime.
- No old-data migration. The final cutover assumes an empty DB/reindex path.
- No Qdrant payload write-path change beyond DTO/schema correlation fields that
  are already supported by PR9.
- No replacement of all existing worker internals in one PR.

## Task 1: Complete Job And Observability DTOs

**Files:**

- Modify: `crates/axon-api/src/source/lifecycle.rs`
- Modify: `crates/axon-api/src/source/status.rs`
- Modify: `crates/axon-api/src/source/stage.rs`
- Modify: `crates/axon-api/src/source/common.rs`
- Modify: `crates/axon-api/src/source/state.rs`
- Modify: `crates/axon-api/src/source/listing.rs`
- Modify: `crates/axon-api/src/source/enums.rs`
- Test: `crates/axon-api/src/source_job_tests.rs`
- Test: `crates/axon-api/src/source_status_tests.rs`
- Update generated artifacts only through `cargo xtask schemas generate`.

**Interfaces:**

- Consumes: `JobKind`, `LifecycleStatus`, `PipelinePhase`, existing ID types.
- Produces: complete target job DTO catalog for `Job`, `JobAttempt`,
  `JobStage`, `SourceProgressEvent`, `JobHeartbeat`, `JobArtifact`,
  `JobResult`, `SourceJobStatus`, list/event/cancel/retry/recover/cleanup
  request and result shapes.

- [ ] Write failing serde/schema tests proving the target DTO catalog exists
  and round-trips:
  - `JobCreateRequest`
  - `JobDescriptor`
  - `JobSummary`
  - `SourceJobStatus`
  - `JobAttemptSnapshot`
  - `JobStageSnapshot`
  - `JobStatusUpdate`
  - `JobEvent`
  - `JobEventListRequest`
  - `JobEventPage`
  - `JobHeartbeat`
  - `JobCancelRequest`
  - `JobCancelResult`
  - `JobRetryRequest`
  - `JobRetryResult`
  - `JobRecoveryRequest`
  - `JobRecoveryResult`
  - `JobCleanupRequest`
  - `JobCleanupResult`
  - `JobArtifactListRequest`
  - `JobArtifactListResult`

- [ ] Write failing tests proving `SourceProgressEvent` contains every
  contract-required field:
  - `event_id`
  - `sequence`
  - `job_id`
  - `attempt`
  - `stage_id`
  - `batch_id`
  - `reservation_id`
  - `checkpoint_id`
  - `dedupe_key`
  - `source_id`
  - `canonical_uri`
  - `adapter`
  - `scope`
  - `generation`
  - `phase`
  - `status`
  - `severity`
  - `visibility`
  - `message`
  - `timestamp`
  - `counts`
  - `timing`
  - `throughput`
  - `current`
  - `retry`
  - `warning`
  - `error`

- [ ] Write failing tests proving `ProgressCurrent` can identify:
  - current source item key
  - document id
  - chunk id
  - adapter
  - provider
  - human-safe message

- [ ] Write failing tests proving event/status enums reject unknown external
  values where the contract requires hard failure, while explicit metadata maps
  continue to allow extension fields.

- [ ] Implement missing DTO fields and builders without duplicating shapes
  already present in `axon-api`.

- [ ] Run `cargo test -p axon-api source_job source_status --locked`.

- [ ] Commit: `feat(api): complete source job observability dtos`.

## Task 2: Implement `axon-observe` Event, Heartbeat, Metric, And Test Sink

**Files:**

- Modify: `crates/axon-observe/src/event.rs`
- Modify: `crates/axon-observe/src/heartbeat.rs`
- Modify: `crates/axon-observe/src/metric.rs`
- Modify: `crates/axon-observe/src/progress.rs`
- Modify: `crates/axon-observe/src/collector.rs`
- Modify: `crates/axon-observe/src/log.rs`
- Modify: `crates/axon-observe/src/span.rs`
- Modify: `crates/axon-observe/src/testing.rs`
- Test: `crates/axon-observe/src/event_tests.rs`
- Test: `crates/axon-observe/src/heartbeat_tests.rs`
- Test: `crates/axon-observe/src/collector_tests.rs`

**Interfaces:**

- Consumes: `SourceProgressEvent`, `JobHeartbeat`, `PipelinePhase`,
  `LifecycleStatus`, `ApiError`.
- Produces: `ObservabilitySink`, no-op sink, in-memory test sink, event
  builders, stage start/finish helpers, heartbeat builders, metric samples,
  redacted structured log fields, span field helpers.

- [ ] Write failing tests for `ObservabilitySink`:
  - `emit` records durable events in order
  - `heartbeat` records latest heartbeat per job
  - `metric` records bounded-label samples
  - `flush` is observable in tests
  - no-op sink accepts all calls

- [ ] Write failing tests for event builders:
  - stage start event sets `phase`, `status=running`, `severity=info`, and
    stage id
  - stage completion event sets terminal stage status and final counts
  - degraded event carries warning payload
  - failed event carries structured `ApiError`
  - provider waiting event uses `status=waiting`

- [ ] Write failing tests for heartbeat builders:
  - foreground heartbeat interval defaults to 5 seconds
  - background heartbeat interval defaults to 15 seconds
  - provider wait/cooling heartbeat includes reservation snapshot
  - heartbeat references last emitted event sequence

- [ ] Write failing tests for structured log helpers:
  - logs include `job_id`, `phase`, `status`, `source_id`, and provider fields
    when available
  - secrets, auth headers, cookies, local home paths, and raw response blobs are
    redacted before becoming log fields

- [ ] Implement `ObservabilitySink`, `NoopObservabilitySink`,
  `InMemoryObservabilitySink`, event builders, heartbeat builders, metric
  samples, and redacted log/span field helpers.

- [ ] Run `cargo test -p axon-observe event heartbeat collector --locked`.

- [ ] Commit: `feat(observe): add event and heartbeat sink`.

## Task 3: Make Provider Reservations Observable And Job-Aware

**Files:**

- Modify: `crates/axon-observe/src/reservation.rs`
- Modify: `crates/axon-observe/src/progress.rs`
- Test: `crates/axon-observe/src/reservation_tests.rs`

**Interfaces:**

- Consumes: provider reservation manager from PR4, `JobId`, `PipelinePhase`,
  `JobStageId`, provider capability classes.
- Produces: reservation ids, job-aware reservation snapshots, cooling events,
  queue/backpressure metrics, heartbeat-ready provider wait summaries.

- [ ] Write failing tests proving each reservation has:
  - `reservation_id`
  - `job_id`
  - `stage_id`
  - provider class
  - lane
  - requested units
  - granted units
  - acquired timestamp
  - expiration timestamp

- [ ] Write failing tests proving cancellation or expiration releases queued and
  acquired reservations without leaking capacity.

- [ ] Write failing tests proving cooling state is visible as:
  - provider class
  - reason
  - started timestamp
  - retry-after timestamp
  - degraded flag
  - queue depth

- [ ] Write failing tests proving interactive ask/query/retrieve capacity is
  reserved when background embedding jobs saturate the background lane.

- [ ] Write failing tests proving reservation snapshots can be embedded in a
  `JobHeartbeat` and provider-wait progress event.

- [ ] Implement job-aware reservation ids and snapshot helpers.

- [ ] Run `cargo test -p axon-observe reservation --locked`.

- [ ] Commit: `feat(observe): expose job-aware provider reservations`.

## Task 4: Harden `JobStore` Fake With State Machine, Events, And Heartbeats

**Files:**

- Modify: `crates/axon-jobs/src/boundary.rs`
- Test: `crates/axon-jobs/src/boundary_tests.rs`
- Create as needed:
  - `crates/axon-jobs/src/state_machine.rs`
  - `crates/axon-jobs/src/state_machine_tests.rs`
  - `crates/axon-jobs/src/event_sequence.rs`
  - `crates/axon-jobs/src/event_sequence_tests.rs`

**Interfaces:**

- Consumes: `axon-api::source` target job DTOs.
- Produces: fake `JobStore` behavior that matches the target contract closely
  enough for source-family ports to use without SQLite.

- [ ] Write failing tests for legal state transitions:
  - `queued -> pending`
  - `queued -> running`
  - `pending -> running`
  - `running -> waiting`
  - `waiting -> running`
  - `running -> canceling`
  - `canceling -> canceled`
  - `running -> completed`
  - `running -> completed_degraded`
  - `running -> failed`
  - `pending -> expired`
  - `queued -> skipped`

- [ ] Write failing tests for illegal transitions:
  - terminal job cannot restart by status update
  - completed job cannot become failed
  - canceled job cannot become running
  - failed job cannot become completed without explicit retry
  - unknown job updates fail with `job.not_found`

- [ ] Write failing tests for per-job event sequencing:
  - first event sequence is 1
  - appended explicit sequence must be next expected sequence
  - duplicate or skipped sequences fail
  - event list honors `after_sequence`, `limit`, severity, and visibility

- [ ] Write failing tests for heartbeat behavior:
  - heartbeat for unknown job fails
  - heartbeat updates latest phase/stage/status/counts without creating events
  - stale heartbeat attempts cannot overwrite a newer attempt
  - provider reservation snapshots round-trip

- [ ] Write failing tests for cancellation/retry/recovery:
  - cancellation records requested reason and moves running jobs to canceling
  - retry creates a linked job descriptor and preserves root/parent ids
  - recovery marks stale running jobs failed or requeued according to policy
  - cleanup removes expired terminal event/heartbeat history according to
    retention policy

- [ ] Implement shared state machine helpers and update `FakeJobWatchStore`.

- [ ] Run `cargo test -p axon-jobs boundary state_machine event_sequence --locked`.

- [ ] Commit: `feat(jobs): harden target job store fake`.

## Task 5: Add SQLite Target Job Store Tables And Repository

**Files:**

- Create: `crates/axon-jobs/src/migrations/0018_unified_jobs_observability.sql`
- Modify: `crates/axon-jobs/src/migrations/checksums.txt`
- Create: `crates/axon-jobs/src/unified.rs`
- Create: `crates/axon-jobs/src/unified_tests.rs`
- Modify: `crates/axon-jobs/src/lib.rs`
- Modify: `crates/axon-jobs/src/store.rs` if migration loading needs the new
  file only.

**Interfaces:**

- Consumes: `JobStore`, target job DTOs, existing SQLite store configuration.
- Produces: SQLite-backed target `JobStore` for source jobs/events/heartbeats
  without replacing legacy family queues yet.

- [ ] Write failing migration tests proving the target tables and indexes exist:
  - `axon_jobs`
  - `axon_job_attempts`
  - `axon_job_stages`
  - `axon_job_events`
  - `axon_job_heartbeats`
  - `axon_job_artifacts`
  - indexes for status/priority/created_at, source_id, watch_id, parent/root
    ids, `(job_id, sequence)`, and heartbeat freshness

- [ ] Write failing tests for SQLite `JobStore::create/get/list`.

- [ ] Write failing tests for SQLite `JobStore::update_status` using the same
  state machine as the fake store.

- [ ] Write failing tests for SQLite `JobStore::append_event`:
  - event sequence is monotonic inside one transaction
  - event details JSON is preserved
  - event pages are deterministic

- [ ] Write failing tests for SQLite `JobStore::heartbeat`:
  - latest heartbeat upserts by `(job_id, attempt)`
  - stale attempts cannot overwrite current attempt
  - heartbeat freshness can be queried for recovery

- [ ] Write failing tests for SQLite recovery/cancellation/cleanup entrypoints
  that mirror the fake store contract.

- [ ] Implement migration, repository, JSON serialization, indexes, and
  checksum validation.

- [ ] Run `cargo test -p axon-jobs unified sqlite_migrations --locked`.

- [ ] Commit: `feat(jobs): add unified sqlite job store`.

## Task 6: Add Service-Level Target Job Operations

**Files:**

- Modify: `crates/axon-services/src/jobs.rs`
- Create as needed: `crates/axon-services/src/source_jobs.rs`
- Test: `crates/axon-services/src/jobs_tests.rs`
- Test: `crates/axon-services/src/source_jobs_tests.rs`

**Interfaces:**

- Consumes: target `JobStore`, current `ServiceContext`, source job DTOs.
- Produces: service functions for target job create/get/list/events/cancel/
  retry/recover/cleanup/artifacts operations, without deleting legacy family
  job operations.

- [ ] Write failing tests proving target job service functions:
  - create source jobs with stage plans
  - return `JobDescriptor` with poll/event/artifact descriptors
  - get status by `job_id`
  - list jobs by kind/status/source
  - list events by sequence
  - cancel jobs cooperatively
  - retry failed/degraded jobs with linked metadata
  - expose cleanup/recovery results

- [ ] Write failing tests proving no public source-family cutover happens in
  this PR:
  - existing legacy job family service tests remain green
  - existing `crawl/embed/extract/ingest` job kind DTOs still deserialize where
    legacy runtime requires them
  - target service functions do not import domain crate internals

- [ ] Implement target service wrappers around `JobStore`.

- [ ] Run `cargo test -p axon-services jobs source_jobs --locked`.

- [ ] Commit: `feat(services): expose target source job operations`.

## Task 7: Bridge Legacy Worker Observability Without Public Cutover

**Files:**

- Modify: `crates/axon-jobs/src/workers/progress.rs`
- Modify: `crates/axon-jobs/src/workers/heartbeat.rs`
- Modify: `crates/axon-jobs/src/workers/starvation.rs`
- Create as needed: `crates/axon-jobs/src/workers/observability.rs`
- Test: `crates/axon-jobs/src/workers/progress_tests.rs`
- Test: `crates/axon-jobs/src/workers/heartbeat_tests.rs`
- Test: `crates/axon-jobs/src/workers/starvation_tests.rs`

**Interfaces:**

- Consumes: `ObservabilitySink`, `SourceProgressEvent`, `JobHeartbeat`,
  existing legacy worker progress data.
- Produces: shared progress/heartbeat conversion helpers that later source
  ports can call, while legacy `progress_json` remains intact.

- [ ] Write failing tests proving legacy progress snapshots can convert to
  `SourceProgressEvent` without losing:
  - job id
  - phase
  - status
  - counts
  - current item
  - warnings
  - error

- [ ] Write failing tests proving legacy heartbeat touch can emit or build a
  target `JobHeartbeat` while preserving the old `updated_at` behavior.

- [ ] Write failing tests proving starvation detector emits a structured
  warning/progress event when a provider/job lane starves.

- [ ] Implement conversion helpers; do not replace current public rendering.

- [ ] Run `cargo test -p axon-jobs workers::progress workers::heartbeat workers::starvation --locked`.

- [ ] Commit: `feat(jobs): bridge legacy worker observability`.

## Task 8: Schema, Docs, And Drift Checks

**Files:**

- Modify generated schema/doc artifacts through `cargo xtask schemas generate`.
- Modify schema generator source only if target job/event/provider DTOs are not
  emitted today.
- Update reference docs only where generated output requires it.

**Interfaces:**

- Consumes: DTOs from Tasks 1-7.
- Produces: fresh event, provider, database, API DTO, and OpenAPI schema
  artifacts for the target job model.

- [ ] Run `cargo xtask schemas generate`.

- [ ] Write or update failing generator tests if new DTOs are missing from:
  - API DTO schema
  - event schema
  - provider capability schema
  - database schema
  - OpenAPI job schema

- [ ] Run `cargo test -p xtask schemas --locked`.

- [ ] Run `cargo xtask schemas generate --check`.

- [ ] Run `cargo xtask check-doc-contracts`.

- [ ] Run `cargo xtask check-doc-links`.

- [ ] Commit: `docs(schemas): refresh unified job observability artifacts`.

## Task 9: Local Verification And PR Gate

- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo test -p axon-api source_job source_status --locked`.
- [ ] Run `cargo test -p axon-observe --locked`.
- [ ] Run `cargo test -p axon-jobs boundary state_machine event_sequence unified --locked`.
- [ ] Run `cargo test -p axon-services jobs source_jobs --locked`.
- [ ] Run `cargo test -p xtask schemas --locked`.
- [ ] Run `cargo xtask schemas generate --check`.
- [ ] Run `cargo xtask check-layering`.
- [ ] Run `cargo xtask check-repo-structure`.
- [ ] Run `cargo xtask check-doc-contracts`.
- [ ] Run `cargo xtask check-doc-links`.
- [ ] Run `cargo xtask check-sqlite-migrations`.
- [ ] Run `git diff --check`.
- [ ] Open or update the PR.
- [ ] Re-read issue #298 and inspect planned PR breakdown item 10
  item-by-item against the PR head.
- [ ] Post the final PR10 checklist audit to issue #298 or the PR.
- [ ] Run `lavra:lavra-review` on the PR and address every introduced issue.
- [ ] Dispatch the PR review toolkit agents against the entire PR and address
  every introduced issue.
- [ ] Re-run relevant tests after review fixes.
- [ ] Wait for required remote checks to go green.
- [ ] Merge PR10 into `main` only after required checks are green and the final
  audit confirms every PR10 item is fully implemented.

