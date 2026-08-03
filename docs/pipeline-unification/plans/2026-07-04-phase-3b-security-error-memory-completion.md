# Security Error Memory Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Phase 3B cutover for fine-grained auth snapshots, typed errors, fail-closed redaction, and first-class Qdrant-backed memory.

**Architecture:** Keep auth, security policy, errors, redaction, jobs, memory, vectors, graph, and transports on their documented ownership boundaries. Jobs execute with immutable enqueue-time auth/config snapshots; memory remains its own job kind, vector namespace/payload family, SQLite metadata store, graph mirror, and retrieval policy path. Public writes pass through a shared redaction report boundary before vector payloads, job events, artifacts, graph evidence, memory rows, CLI JSON, MCP responses, REST responses, or traces are emitted.

**Tech Stack:** Rust 2024 workspace, `axon-api`, `axon-authz`, `axon-error`, `axon-core`, `axon-jobs`, `axon-memory`, `axon-vectors`, `axon-graph`, `axon-services`, `axon-cli`, `axon-mcp`, `axon-web`, SQLite, Qdrant, `serde`, `schemars`, `utoipa`, Tokio tests.

## Global Constraints

- Source of truth: `docs/pipeline-unification/runtime/auth-contract.md`, `security-contract.md`, `error-handling.md`, `redaction-contract.md`, `memory-contract.md`, and `job-contract.md`.
- Memory is Qdrant/vector-backed with SQLite metadata and graph mirrors; it is not a source adapter and must not pollute normal source retrieval without explicit memory search/context intent.
- `axon:write` does not imply `axon:admin`, `axon:execute`, or `axon:local`.
- Workers must run with the enqueue-time `auth_snapshot`, not current process defaults.
- Redaction failure fails closed before vector/event/artifact/graph/memory/transport writes.
- Every public payload write records `redaction_status`, `redaction_version`, `visibility`, redacted field count, dropped field count, and detector names.
- Removed or legacy job stores must not be silently reused after the full durable job cutover; non-empty old stores block unified workers and destructive reset until an explicit admin reset/clear receipt exists. Do not support legacy job import as a clean-break path.
- Admin/destructive routes must require `axon:admin`; `axon:write` is insufficient for reset, prune, provider config, local, execute, or destructive cleanup.
- Redaction failure tests must use writer spies and prove zero writes to vector, job event, artifact, graph, memory, CLI JSON, MCP response, REST response, and trace sinks.
- Memory search must batch metadata loads with `load_many(memory_ids)` and preserve vector result order. Do not perform one SQLite query per vector hit.
- Newly remembered memory must not be recallable until SQLite metadata, vector refs, and graph mirror transaction state are committed or explicitly recoverable.
- Memory retrieval must require explicit memory intent and memory namespace filters; memory must not pollute normal source retrieval.
- Use `apply_patch` for manual file edits and keep unrelated user work intact.

---

## Engineering Review Corrections

This plan is at least three implementation tracks. Keep cutover blockers first:

```text
auth/error/redaction enforcement
memory vector/SQLite/graph core and status rules
memory lifecycle/import/export/review/compact enhancements
```

Tasks that mutate unified jobs depend on the full durable job store landing first. Mark them blocked or split them out if the job cutover is not already merged.

## File Structure

- Modify `crates/axon-api/src/source/auth.rs`: add transport-neutral `CallerContext`, `AuthSnapshot`, `AuthScope`, `AuthMode`, and `TransportKind` DTOs if missing from the source contract module.
- Modify `crates/axon-api/src/source/job.rs` or the existing job DTO module that defines `JobCreateRequest`: replace raw `MetadataMap auth_snapshot` with typed `AuthSnapshot` and add `ApiError` fields for job/stage/event projections.
- Modify `crates/axon-api/src/source/memory.rs`: add missing memory DTOs for update, pin, archive, forget, compact, import/export, batch boundaries, context exclusions, and vector/graph refs.
- Modify `crates/axon-api/src/source/redaction.rs` or create it if absent: define serializable `RedactionReport`, `RedactionStatus`, `RedactionSurface`, and `RedactionContext` shared by writers.
- Modify `crates/axon-authz/src/lib.rs` and `crates/axon-authz/src/policy.rs`: enforce read/write/admin/execute/local scope separation and caller-to-snapshot conversion.
- Modify `crates/axon-error/src/*`: finish `ApiError` propagation helpers, provider cooling/retry fields, redaction failure constructors, and item-level error constructors.
- Modify `crates/axon-jobs/src/unified/*.rs`: persist typed auth snapshots, propagate `ApiError` through job rows/events/stages, reject invalid transitions without mutation, and enforce old-store blockers/reset behavior.
- Modify `crates/axon-jobs/src/watch*.rs` and `crates/axon-jobs/src/workers/**/*.rs`: pass auth snapshots through watch execution, retry, stale reclaim, child jobs, prune, reset, local, execute, and memory jobs.
- Modify `crates/axon-core/src/redaction.rs` or create it if the shared boundary is absent; keep `crates/axon-vectors/src/redactor.rs` as an adapter over the shared API instead of the owner.
- Modify `crates/axon-vectors/src/ops/tei/pipeline/payload.rs`, `crates/axon-vectors/src/ops/tei/qdrant_store.rs`, and redaction tests: emit full redaction report data and reject unsafe writes.
- Modify `crates/axon-memory/src/{lib.rs,store.rs,sqlite.rs,vector.rs,graph.rs,context.rs,redaction.rs,review.rs,compact.rs}`: complete memory lifecycle, vector integration, graph mirror, status rules, batch boundaries, and recovery.
- Modify `crates/axon-services/src/memory.rs`: stop treating memory as SQLite-only; route all operations through the completed `MemoryService`.
- Modify `crates/axon-cli/src/commands/memory.rs`, `crates/axon-mcp/src/**/memory*.rs`, and `crates/axon-web/src/**/memory*.rs`: expose the same DTOs and status rules across CLI/MCP/REST.
- Add fixtures under `crates/axon-error/tests/fixtures/schema/`, `crates/axon-memory/fixtures/`, `crates/axon-vectors/tests/fixtures/payload/`, and `crates/axon-graph/fixtures/`.

---

### Task 1: Typed Auth Snapshot And Scope Enforcement

**Files:**
- Modify: `crates/axon-api/src/source/auth.rs`
- Modify: `crates/axon-api/src/source.rs`
- Modify: `crates/axon-authz/src/lib.rs`
- Modify: `crates/axon-authz/src/policy.rs`
- Test: `crates/axon-authz/src/lib_tests.rs`
- Test: `crates/axon-jobs/src/auth_snapshot_tests.rs`

**Interfaces:**
- Consumes: scope constants `AXON_READ_SCOPE`, `AXON_WRITE_SCOPE`, `AXON_ADMIN_SCOPE`, `AXON_EXECUTE_SCOPE`, `AXON_LOCAL_SCOPE`.
- Produces: `CallerContext`, `AuthSnapshot`, `AuthScope`, `AuthMode`, `TransportKind`, `required_scope_for_operation(operation: OperationClass) -> AuthScope`, and `AuthSnapshot::from_caller(&CallerContext, Visibility, &str)`.

- [ ] **Step 1: Add failing auth DTO tests**

Add tests that prove broad write does not satisfy fine-grained admin/execute/local and that snapshots preserve enqueue-time grants:

```rust
#[test]
fn write_scope_does_not_satisfy_fine_grained_scopes() {
    let scopes = vec![AXON_WRITE_SCOPE.to_string()];
    assert!(!scope_satisfies(&scopes, AXON_ADMIN_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
}

#[test]
fn auth_snapshot_is_immutable_projection_of_caller() {
    let caller = CallerContext {
        caller_id: Some("user_1".to_string()),
        transport: TransportKind::Mcp,
        trusted_local: false,
        scopes: vec![AuthScope::Read, AuthScope::Write],
        auth_mode: AuthMode::Oauth,
        token_id: Some("tok_1".to_string()),
        display_name: Some("Jacob".to_string()),
    };
    let snapshot = AuthSnapshot::from_caller(&caller, Visibility::Public, "2026-07-04");
    assert_eq!(snapshot.caller_id.as_deref(), Some("user_1"));
    assert_eq!(snapshot.transport, TransportKind::Mcp);
    assert_eq!(snapshot.scopes, vec![AuthScope::Read, AuthScope::Write]);
    assert_eq!(snapshot.visibility_ceiling, Visibility::Public);
    assert_eq!(snapshot.policy_version, "2026-07-04");
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-authz auth_snapshot write_scope --no-fail-fast`

Expected: tests referencing new DTOs fail to compile or fail because snapshot projection is missing.

- [ ] **Step 3: Implement typed auth DTOs**

Add the DTOs with `serde`, `schemars`, and `utoipa` derives:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthScope { Read, Write, Admin, Execute, Local }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind { Cli, Rest, Mcp, Worker, Scheduler }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode { None, TrustedLocal, StaticToken, Oauth, Test }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CallerContext {
    pub caller_id: Option<String>,
    pub transport: TransportKind,
    pub trusted_local: bool,
    pub scopes: Vec<AuthScope>,
    pub auth_mode: AuthMode,
    pub token_id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthSnapshot {
    pub caller_id: Option<String>,
    pub transport: TransportKind,
    pub granted_scopes: Vec<AuthScope>,
    pub visibility_ceiling: Visibility,
    pub request_time: Timestamp,
    pub policy_version: String,
}
```

Implement conversion helpers without reading current config after enqueue.

- [ ] **Step 4: Run auth tests**

Run: `cargo test -p axon-authz --no-fail-fast`

Expected: auth scope and snapshot tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/axon-api/src/source/auth.rs crates/axon-api/src/source.rs crates/axon-authz/src/lib.rs crates/axon-authz/src/policy.rs crates/axon-authz/src/lib_tests.rs crates/axon-jobs/src/auth_snapshot_tests.rs
git commit -m "feat: add typed auth snapshots"
```

---

### Task 2: Job Snapshot Enforcement Across Workers

**Files:**
- Modify: `crates/axon-api/src/source/job.rs` or existing job DTO file
- Modify: `crates/axon-jobs/src/unified/control.rs`
- Modify: `crates/axon-jobs/src/unified/ops.rs`
- Modify: `crates/axon-jobs/src/watch.rs`
- Modify: `crates/axon-jobs/src/workers/**/*.rs`
- Modify: `crates/axon-services/src/jobs.rs`
- Test: `crates/axon-jobs/src/auth_snapshot_tests.rs`
- Test: `crates/axon-services/src/jobs_auth_tests.rs`

**Interfaces:**
- Consumes: `AuthSnapshot` from Task 1.
- Produces: `JobCreateRequest.auth_snapshot: AuthSnapshot`, `WorkerExecutionContext.auth_snapshot: AuthSnapshot`, and `PolicyDecision::deny(ApiError)`.

- [ ] **Step 1: Add failing job auth snapshot tests**

Add tests for watch child jobs, stale reclaim, and local/execute denial:

```rust
#[tokio::test]
async fn child_job_inherits_parent_auth_snapshot() {
    let store = store().await;
    let parent = store.create(watch_job_with_scopes(vec![AuthScope::Read, AuthScope::Write])).await.unwrap();
    let child = create_child_source_job(&store, parent.job_id).await.unwrap();
    assert_eq!(child.auth_snapshot.granted_scopes, vec![AuthScope::Read, AuthScope::Write]);
}

#[tokio::test]
async fn stale_reclaim_does_not_gain_new_local_scope() {
    let store = store().await;
    let job = store.create(local_source_job_with_scopes(vec![AuthScope::Read, AuthScope::Write])).await.unwrap();
    let decision = reclaim_and_authorize(&store, job.job_id, AuthScope::Local).await;
    assert_eq!(decision.unwrap_err().code.to_string(), "auth.scope_required");
}

#[tokio::test]
async fn execute_job_without_execute_scope_fails_before_side_effect() {
    let store = store().await;
    let job = store.create(cli_tool_job_with_scopes(vec![AuthScope::Write])).await.unwrap();
    let outcome = run_tool_job_until_authorization(&store, job.job_id).await;
    assert_eq!(outcome.unwrap_err().stage, ErrorStage::Authorizing);
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-jobs auth_snapshot --no-fail-fast`

Expected: tests fail because typed snapshots are not persisted/enforced everywhere.

- [ ] **Step 3: Persist typed snapshots and pass them into workers**

Replace `MetadataMap` snapshot plumbing with typed JSON serialization at the store boundary. Add a worker execution context:

```rust
pub struct WorkerExecutionContext {
    pub job_id: JobId,
    pub attempt: u32,
    pub auth_snapshot: AuthSnapshot,
    pub config_snapshot_id: ConfigSnapshotId,
    pub cancellation: CancellationToken,
}
```

Every worker entry loads `JobSummary.auth_snapshot` before resolving credentials, local paths, tools, prune/reset selectors, child jobs, retries, and stale recovery. Child job creation copies the parent snapshot unless the caller explicitly submits a new authenticated request.

- [ ] **Step 4: Enforce operation-specific policy before side effects**

Add a single enforcement helper and use it in watch execution, retry, stale reclaim, child job creation, prune, reset, local source, CLI tool, MCP tool, and memory jobs:

```rust
pub fn require_job_scope(snapshot: &AuthSnapshot, required: AuthScope) -> Result<(), ApiError> {
    if snapshot.granted_scopes.contains(&required) {
        return Ok(());
    }
    Err(ApiError::new(
        "auth.scope_required",
        ErrorStage::Authorizing,
        format!("operation requires {:?}", required),
    ).with_visibility(ErrorVisibility::Public))
}
```

- [ ] **Step 5: Run job/service tests**

Run:

```bash
cargo test -p axon-jobs auth_snapshot --no-fail-fast
cargo test -p axon-services jobs_auth --no-fail-fast
```

Expected: watch/retry/reclaim/child/local/execute enforcement tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-api/src crates/axon-jobs/src crates/axon-services/src
git commit -m "feat: enforce enqueue-time auth snapshots"
```

---

### Task 3: ApiError Propagation And Provider Cooling

**Files:**
- Modify: `crates/axon-error/src/api_error.rs`
- Modify: `crates/axon-error/src/cooling.rs`
- Modify: `crates/axon-error/src/retry.rs`
- Modify: `crates/axon-api/src/source/error.rs` or existing source error projection module
- Modify: `crates/axon-jobs/src/unified/control.rs`
- Modify: `crates/axon-jobs/src/unified/observe.rs`
- Modify: `crates/axon-services/src/**/*.rs`
- Test: `crates/axon-error/src/api_error_tests.rs`
- Test: `crates/axon-jobs/src/unified_tests.rs`

**Interfaces:**
- Consumes: `ApiError`, `ProviderCooling`, `RetryPolicy`, `ErrorStage`.
- Produces: `ApiError::with_retry_policy`, `ApiError::with_provider_cooling`, `SourceItemError`, and job event error projection.

- [ ] **Step 1: Add failing propagation tests**

Add tests that assert job events preserve typed error fields:

```rust
#[tokio::test]
async fn job_event_contains_structured_api_error() {
    let store = store().await;
    let job = store.create(create_request()).await.unwrap();
    let error = ApiError::new("provider.cooling", ErrorStage::Provider, "provider cooling")
        .with_retry_after_ms(30_000)
        .with_cooldown_until(Timestamp("2026-07-04T12:30:00Z".to_string()));
    store.update_status(JobStatusUpdate {
        job_id: job.job_id,
        status: LifecycleStatus::Waiting,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: None,
        current: None,
        message: Some("waiting on provider".to_string()),
        error: Some(error.clone()),
    }).await.unwrap();
    let events = store.events(JobEventListRequest::for_job(job.job_id)).await.unwrap();
    let event_error = events.events.last().unwrap().error.as_ref().unwrap();
    assert_eq!(event_error.code.to_string(), "provider.cooling");
    assert_eq!(event_error.retry_after_ms, Some(30_000));
    assert_eq!(event_error.cooldown_until.as_ref().unwrap().0, "2026-07-04T12:30:00Z");
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-error api_error --no-fail-fast && cargo test -p axon-jobs job_event_contains_structured_api_error --no-fail-fast`

Expected: compile or assertion failure around missing event error projection.

- [ ] **Step 3: Implement error projection**

Ensure every transport-neutral error includes:

```rust
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub stage: ErrorStage,
    pub retryable: bool,
    pub severity: ErrorSeverity,
    pub visibility: ErrorVisibility,
    pub details: serde_json::Map<String, serde_json::Value>,
    pub job_id: Option<String>,
    pub source_id: Option<String>,
    pub source_item_key: Option<String>,
    pub document_id: Option<String>,
    pub chunk_id: Option<String>,
    pub provider_id: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub cooldown_until: Option<Timestamp>,
}
```

Add `SourceItemError` for item-level failures with source id, item key, generation, status, code, stage, retryable, attempt, and redacted details.

- [x] **Step 4: Wire provider cooling into jobs**

When a provider is cooling, transition the job to `waiting`, append a public event with the typed `ApiError`, and prevent hot loops by requiring a scheduler reservation after `cooldown_until`.

**Evidence (closed out via `docs/pipeline-unification/plans/2026-07-08-provider-cooling.md`, branch `provider-cooling-impl`):**
- `crates/axon-jobs/src/migrations/0022_add_job_cooldown_until.sql` — adds `jobs.cooldown_until` (TEXT/RFC3339) plus a covering partial index (`idx_axon_jobs_claim_cooldown`, `WHERE status IN ('queued', 'waiting', 'blocked')` — widened to match the claim query's exact status set after an initial `WHERE status = 'waiting'` version was found to defeat SQLite's partial-index containment matcher and fall back to a full table scan) in the same migration. Commit `2f2ce4c13` ("feat(jobs): add cooldown_until column with covering index").
- `crates/axon-jobs/src/unified/control.rs::apply_provider_cooling` — persists a bounded cooldown (`min(cooldown_until, now + MAX_PROVIDER_COOLDOWN_WINDOW)`, fixed at 1 hour, `crates/axon-jobs/src/unified.rs::MAX_PROVIDER_COOLDOWN_WINDOW`) on a job currently in `Waiting`, inside a transaction with a `status = 'waiting'`-guarded UPDATE so a concurrent transition off `Waiting` between the check and the write can't leave a stale cooldown.
- `crates/axon-jobs/src/workers/unified.rs::claim_next_unified_job_unchecked` — claim query now excludes rows whose `cooldown_until` is still in the future (`AND (cooldown_until IS NULL OR cooldown_until <= ?)`), index-covered by the migration above.
- `cooldown_until` is cleared on every transition to a non-`Waiting` status: unconditionally in `SqliteUnifiedJobStore::update_job_status` (`crates/axon-jobs/src/unified/ops.rs`) via a `CASE WHEN ... = 'waiting' THEN cooldown_until ELSE NULL END`, and independently in the other raw-SQL `jobs` writers that bypass that function — `mark_terminal` and the claim-time `UPDATE ... SET status = 'running'` (both in `crates/axon-jobs/src/workers/unified.rs`), `cancel_job` (`crates/axon-jobs/src/unified/control.rs`), `record_heartbeat` (`crates/axon-jobs/src/unified/heartbeat.rs`), and `reset_job_for_retry`/`reset_stale_job_for_recovery` (`crates/axon-jobs/src/unified/control_helpers.rs`).
- Commit `365d21a63` ("feat(jobs): bound and wire provider cooling into job claim eligibility") — full test coverage in `crates/axon-jobs/src/provider_cooling_tests.rs` (clamping, claim exclusion/eligibility, clear-on-completion, clear-on-terminal-failure, past-deadline round-trip, Waiting-only guard).
- **Known gap, intentionally not closed here:** no live `UnifiedJobRunner` in the composition layer (`crates/axon-services/src/runtime/job_runners.rs`) currently calls a provider (TEI/LLM) and constructs `ApiError::with_provider_cooling(...)` — TEI's 429/5xx retry-exhaustion path (`crates/axon-vector/src/ops/tei/tei_client.rs::send_chunk_with_retries`) still returns a plain `Box<dyn Error>` and is only reachable through the legacy per-family embed job runner (`crates/axon-jobs/src/workers/runners/embed.rs`, `axon_embed_jobs` table), not the unified `jobs` table this work targets. The generic mechanism (bounded clamp, claim exclusion, clear-on-terminal) is real and tested end-to-end; wiring an actual TEI-calling `UnifiedJobRunner` to call `apply_provider_cooling` is follow-up work, not fabricated here to avoid inventing an unverified call site.

- [ ] **Step 5: Run error and job tests**

Run:

```bash
cargo test -p axon-error --no-fail-fast
cargo test -p axon-jobs provider cooling error --no-fail-fast
```

Expected: structured error, retry, cooling, and item-level tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-error/src crates/axon-api/src crates/axon-jobs/src crates/axon-services/src
git commit -m "feat: propagate structured pipeline errors"
```

---

### Task 4: Shared Redaction Boundary And Fail-Closed Public Writes

> **PARTIALLY DONE** (2026-07-09) — file-log and artifact-metadata redaction
> closed; CLI JSON gate covers 4/70 call sites, remainder tracked in bead
> `axon_rust-l6amm`. The shared boundary already covered vector payloads,
> job events, graph evidence, memory rows, and MCP/REST error responses. Of
> the three remaining chokepoints — CLI JSON output, artifact metadata writes,
> and trace/log fields — artifact metadata and trace/log fields are now fully
> closed, and CLI JSON output has a shared gate (`crate::json::print_json_gated`
> in `crates/axon-cli/src/json.rs`) wired into 4 call sites
> (`common_jobs.rs`, `memory.rs`, and the `watch history`/`watch artifacts`
> reads in `watch.rs`) out of roughly 70 `serde_json::to_string_pretty`/
> `to_string` call sites across `crates/axon-cli/src/commands/*.rs`. See
> [`docs/pipeline-unification/plans/2026-07-08-redaction-boundary-extension.md`](2026-07-08-redaction-boundary-extension.md):
> `RedactionContext::cli_json()` gating the CLI `--json` render path
> (`crates/axon-cli/src/json.rs`), `RedactionContext::artifact_metadata()`
> gating the previously-ungated `watch` `url-change` artifact write
> (`crates/axon-jobs/src/watch/report.rs`), and a new `JsonFormat` event
> formatter (`crates/axon-core/src/logging/json_format.rs`) closing a gap
> where the file JSON log sink bypassed the console layer's
> `redact_event_fields` scrub — that scrub now also covers span-level fields,
> not just event-level fields, and caps oversized field values instead of
> scanning them unbounded. The actual gate API differs from this task's
> illustrative sketch below — see the extension plan's Task 1 findings for the
> real function names/signatures. **Caveat:** the extension plan's Task 4 was
> an explicit best-effort manual-grep pass over trace/log call sites, not a
> completeness guarantee; CI/lint enforcement and the remaining CLI JSON
> call-site retrofit are tracked separately as bead
> `axon_rust-l6amm` ("Add CI-enforced redaction call-site lint").

**Files:**
- Create: `crates/axon-core/src/redaction.rs` if absent
- Modify: `crates/axon-core/src/lib.rs`
- Modify: `crates/axon-api/src/source/redaction.rs` or create it
- Modify: `crates/axon-vectors/src/redactor.rs`
- Modify: `crates/axon-vectors/src/ops/tei/pipeline/payload.rs`
- Modify: `crates/axon-jobs/src/unified/observe.rs`
- Modify: `crates/axon-memory/src/redaction.rs`
- Test: `crates/axon-vectors/src/redactor_tests.rs`
- Test: `crates/axon-memory/src/redaction_tests.rs`
- Test: `crates/axon-services/src/redaction_boundary_tests.rs`

**Interfaces:**
- Consumes: `RedactionContract` requirements.
- Produces: shared `Redactor` trait, `RedactionReport`, `RedactionFailure`, and `redact_public_write(surface, value, context) -> Result<RedactedValue>`.

- [ ] **Step 1: Add failing fail-closed tests**

Add a fixture-driven test for every public write surface:

```rust
#[test]
fn vector_payload_secret_fixture_fails_before_write() {
    let payload = secret_payload_fixture();
    let err = validate_and_redact_public_write(RedactionSurface::VectorPayload, payload).unwrap_err();
    assert_eq!(err.code.to_string(), "redaction.failed");
    assert_eq!(err.stage, ErrorStage::Authorizing);
}

#[test]
fn redaction_report_contains_contract_fields() {
    let report = redact_fixture("authorization: bearer abcdef0123456789abcdef").unwrap();
    assert_eq!(report.redaction_version, "2026-07-04");
    assert_eq!(report.visibility, Visibility::Public);
    assert_eq!(report.redacted_field_count, 1);
    assert_eq!(report.dropped_field_count, 0);
    assert_eq!(report.detector_names, vec!["bearer_token"]);
}
```

Cover vector payloads, job events, artifacts, graph evidence, memory rows, CLI JSON, MCP responses, REST responses, and traces with one fixture each.

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-vectors redaction --no-fail-fast`

Expected: existing vector redaction tests pass partially, but new report fields and cross-surface failures are missing.

- [ ] **Step 3: Move redaction ownership to `axon-core`**

Define shared DTOs in `axon-api` and implementation in `axon-core`:

```rust
pub struct RedactionReport {
    pub redaction_status: RedactionStatus,
    pub redaction_version: String,
    pub visibility: Visibility,
    pub redacted_field_count: u32,
    pub dropped_field_count: u32,
    pub detector_names: Vec<String>,
}
```

Keep `axon-vectors::redactor` as a compatibility adapter that calls the shared redactor. Do not let vector-only logic own the public security boundary.

- [ ] **Step 4: Gate all public writes**

Require `RedactionReport` before:

```text
VectorStore upsert
JobStore append_event
ArtifactStore public metadata write
SourceGraph public evidence write
MemoryStore body/metadata write
CLI JSON render for untrusted mode
MCP response envelope
REST response envelope
trace/log fields crossing public visibility
```

If redaction returns `Failed`, return `ApiError::new("redaction.failed", ErrorStage::Authorizing, "...")` and do not call the downstream writer.

- [ ] **Step 5: Run redaction tests**

Run:

```bash
cargo test -p axon-vectors redaction --no-fail-fast
cargo test -p axon-memory redaction --no-fail-fast
cargo test -p axon-services redaction --no-fail-fast
```

Expected: all public write fixtures prove fail-closed behavior and report fields.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-api/src crates/axon-core/src crates/axon-vectors/src crates/axon-jobs/src crates/axon-memory/src crates/axon-services/src
git commit -m "feat: enforce shared redaction boundary"
```

---

### Task 5: Memory Vector Store And Payload Family

**Files:**
- Create: `crates/axon-memory/src/vector.rs`
- Modify: `crates/axon-memory/src/lib.rs`
- Modify: `crates/axon-memory/src/store.rs`
- Modify: `crates/axon-memory/src/sqlite.rs`
- Modify: `crates/axon-vectors/src/ops/tei/pipeline/payload.rs`
- Modify: `docs/pipeline-unification/sources/metadata-payload.md` only if implementation discovers a missing approved memory field from the contract
- Test: `crates/axon-memory/src/vector_tests.rs`
- Test: `crates/axon-vectors/tests/fixtures/payload/memory.valid.json`

**Interfaces:**
- Consumes: `MemoryRequest`, `MemoryRecord`, `EmbeddingProvider`, `VectorStore`, `Redactor`.
- Produces: `MemoryVectorStore`, `MemoryVectorPayload`, `MemoryEmbeddingRef`, and Qdrant filters for `vector_namespace="memory"`.

- [ ] **Step 1: Add failing memory vector tests**

Add tests proving remember writes metadata plus vector refs and forgotten memory is not recallable:

```rust
#[tokio::test]
async fn remember_writes_memory_vector_payload() {
    let service = memory_service_with_fake_vector_store().await;
    let result = service.remember(valid_memory_request()).await.unwrap();
    assert_eq!(result.vector_point_ids.len(), 1);
    let payload = service.fake_vectors().last_payload().unwrap();
    assert_eq!(payload["vector_namespace"], "memory");
    assert_eq!(payload["memory_id"], result.memory_id.0);
    assert_eq!(payload["memory_status"], "active");
    assert_eq!(payload["redaction_status"], "clean");
}

#[tokio::test]
async fn forgotten_memory_deletes_or_hides_vector_points() {
    let service = memory_service_with_fake_vector_store().await;
    let result = service.remember(valid_memory_request()).await.unwrap();
    service.forget(MemoryForgetRequest { memory_id: result.memory_id.clone(), reason: "test".into() }).await.unwrap();
    let hits = service.search(MemorySearchRequest { query: "durable".into(), limit: 10, filters: Default::default(), include_graph: false, include_archived: false, reinforce: false }).await.unwrap();
    assert!(hits.results.iter().all(|hit| hit.record.memory_id != result.memory_id));
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-memory vector --no-fail-fast`

Expected: tests fail because current memory service is SQLite keyword recall and graph/vector modules are marker boundaries.

- [ ] **Step 3: Implement memory vector boundary**

Create `MemoryVectorStore`:

```rust
#[async_trait]
pub trait MemoryVectorStore: Send + Sync {
    async fn upsert_memory(&self, record: &MemoryRecord, report: &RedactionReport) -> Result<Vec<VectorPointId>>;
    async fn search_memory(&self, request: &MemorySearchRequest) -> Result<Vec<MemoryVectorHit>>;
    async fn delete_memory_vectors(&self, memory_id: &MemoryId) -> Result<()>;
    async fn update_memory_status(&self, memory_id: &MemoryId, status: MemoryStatus) -> Result<()>;
}
```

Payload fields must include `vector_namespace="memory"`, `memory_id`, `memory_type`, `memory_status`, `memory_scope_kind`, `memory_scope_value`, `memory_confidence`, `memory_salience`, `redaction_status`, `redaction_version`, `visibility`, `job_id`, embedding model/provider/profile, and redaction report counts/detectors.

- [ ] **Step 4: Replace keyword-only recall with vector recall**

`MemoryStore` remains SQLite metadata; `MemoryService` composes SQLite plus vector plus graph:

```text
remember -> redact -> store SQLite row -> embed/upsert memory vector -> update embedding_refs -> mirror graph node
search -> vector search memory namespace -> load SQLite records -> apply status/auth/decay/scope scoring
context -> memory search -> token budget assembly -> citations/exclusions
forget -> mark forgotten -> delete or hide vectors -> redact graph recall edges
```

- [ ] **Step 5: Run memory/vector payload tests**

Run:

```bash
cargo test -p axon-memory vector --no-fail-fast
cargo test -p axon-vectors memory payload --no-fail-fast
```

Expected: memory vector payload fixture validates and search uses memory namespace only.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-memory/src crates/axon-vectors/src crates/axon-vectors/tests/fixtures/payload
git commit -m "feat: add vector-backed memory recall"
```

---

### Task 6: Memory Graph Mirror And Status Rules — DONE

**Status (P1-V01, confirmed 2026-07-09 audit):** fully implemented — `MemoryGraphMirror`
is a real trait/impl in `crates/axon-memory/src/graph.rs` (not a marker
module), with `upsert_memory_node`/`supersedes`/`contradicts` wired. Checkboxes
below were stale (unchecked despite landed code) and are now checked.

**Files:**
- Create: `crates/axon-memory/src/graph.rs` implementation replacing marker module
- Modify: `crates/axon-memory/src/link.rs`
- Modify: `crates/axon-memory/src/sqlite/lifecycle.rs`
- Modify: `crates/axon-graph/src/**` where graph memory evidence DTOs live
- Test: `crates/axon-memory/src/graph_tests.rs`
- Test: `crates/axon-graph/fixtures/memory-links.valid.json`

**Interfaces:**
- Consumes: `GraphStore`, `MemoryLink`, `MemoryStatus`.
- Produces: `MemoryGraphMirror`, memory nodes, `supersedes`, `contradicts`, `derived_from`, and evidence-backed scope links.

- [x] **Step 1: Add failing graph/status tests**

Add tests for graph mirror operations and recall exclusions:

```rust
#[tokio::test]
async fn supersede_hides_old_memory_and_writes_graph_edge() {
    let service = memory_service_with_fake_graph().await;
    let old = service.remember(memory("old decision")).await.unwrap();
    let new = service.remember(memory("new decision")).await.unwrap();
    service.supersede(MemorySupersedeRequest { memory_id: old.memory_id.clone(), replacement_id: new.memory_id.clone(), reason: Some("changed".into()), timestamp: now() }).await.unwrap();
    assert!(service.search(default_search("old")).await.unwrap().results.is_empty());
    assert!(service.fake_graph().has_edge(&new.memory_id.0, "supersedes", &old.memory_id.0));
}

#[tokio::test]
async fn contradicted_memory_returns_with_warning() {
    let service = memory_service_with_fake_graph().await;
    let a = service.remember(memory("qdrant is remote")).await.unwrap();
    let b = service.remember(memory("qdrant is local")).await.unwrap();
    service.contradict(MemoryContradictRequest { memory_id: a.memory_id.clone(), conflicting_id: b.memory_id.clone(), reason: Some("conflict".into()), timestamp: now() }).await.unwrap();
    let result = service.search(default_search("qdrant")).await.unwrap();
    assert!(result.warnings.iter().any(|w| w.code == "memory.contradicted"));
}
```

- [x] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-memory graph supersede contradicted --no-fail-fast`

Expected: tests fail because graph mirror is a marker and status behavior is incomplete in the composed service.

- [x] **Step 3: Implement `MemoryGraphMirror`**

Add:

```rust
#[async_trait]
pub trait MemoryGraphMirror: Send + Sync {
    async fn upsert_memory_node(&self, record: &MemoryRecord, evidence: MemoryGraphEvidence) -> Result<Option<GraphNodeId>>;
    async fn link_scope(&self, memory_id: &MemoryId, scope: &MemoryScope, evidence: MemoryGraphEvidence) -> Result<()>;
    async fn supersedes(&self, replacement_id: &MemoryId, old_id: &MemoryId, evidence: MemoryGraphEvidence) -> Result<()>;
    async fn contradicts(&self, left: &MemoryId, right: &MemoryId, evidence: MemoryGraphEvidence) -> Result<()>;
    async fn derived_from(&self, compacted_id: &MemoryId, source_ids: &[MemoryId], evidence: MemoryGraphEvidence) -> Result<()>;
    async fn hide_recall_edges(&self, memory_id: &MemoryId, reason: &str) -> Result<()>;
}
```

Graph evidence includes memory id, job/request id, caller, timestamp, reason, visibility, and redaction report.

- [x] **Step 4: Enforce status rules in search/context**

Implement these rules in one predicate used by search and context:

```text
forgotten: never returns
archived: excluded unless include_archived=true
superseded: excluded unless explicit include_superseded=true request field exists
contradicted: may return with warning and contradiction penalty
review: may return with lower confidence and warning
pinned: minimum score floor, still respects auth/redaction
```

- [x] **Step 5: Run graph/status tests**

Run:

```bash
cargo test -p axon-memory graph status --no-fail-fast
cargo test -p axon-graph memory --no-fail-fast
```

Expected: graph mirror fixture and status rules pass.

- [x] **Step 6: Commit**

Run:

```bash
git add crates/axon-memory/src crates/axon-graph crates/axon-graph/fixtures
git commit -m "feat: mirror memory lifecycle into graph"
```

---

### Task 7: Complete Memory Lifecycle Surface

**Files:**
- Modify: `crates/axon-api/src/source/memory.rs`
- Create: `crates/axon-memory/src/compact.rs`
- Create: `crates/axon-memory/src/reinforce.rs`
- Create: `crates/axon-memory/src/supersede.rs`
- Create: `crates/axon-memory/src/contradict.rs`
- Modify: `crates/axon-memory/src/review.rs`
- Modify: `crates/axon-memory/src/context.rs`
- Modify: `crates/axon-services/src/memory.rs`
- Test: `crates/axon-memory/src/lifecycle_tests.rs`
- Test: `crates/axon-services/src/memory_tests.rs`

**Interfaces:**
- Consumes: `MemoryService` contract from `runtime/memory-contract.md`.
- Produces: remember, get/show, search, context, link, update, reinforce, supersede, contradict, pin, archive, forget, review, compact, import, export, scope graph links, decay profiles, review queues, contradiction penalties, and token-budget context assembly.

- [ ] **Step 1: Add failing service-surface tests**

Write one test per required operation using DTOs:

```rust
#[tokio::test]
async fn service_exposes_required_memory_operations() {
    let service = memory_service_with_fakes().await;
    let created = service.remember(memory("phase 3b uses qdrant memory")).await.unwrap();
    service.update(update_body(&created.memory_id, "phase 3b keeps qdrant memory")).await.unwrap();
    service.reinforce(reinforce(&created.memory_id, 0.2)).await.unwrap();
    service.pin(pin(&created.memory_id, true)).await.unwrap();
    service.archive(archive(&created.memory_id)).await.unwrap();
    service.review(MemoryReviewRequest::default()).await.unwrap();
    service.forget(forget(&created.memory_id)).await.unwrap();
}

#[tokio::test]
async fn context_reports_budget_exclusions() {
    let service = memory_service_with_fakes().await;
    seed_many_memories(&service).await;
    let result = service.context(MemoryContextRequest { token_budget: 20, query: Some("pipeline".into()), source_id: None, graph_node_id: None, filters: Default::default(), depth: None, include_working: false }).await.unwrap();
    assert!(result.token_estimate <= 20);
    assert!(result.exclusions.iter().any(|e| e.contains("budget")));
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-memory lifecycle context --no-fail-fast`

Expected: tests fail for missing DTOs/operations or incomplete service behavior.

- [ ] **Step 3: Add missing DTOs and service trait methods**

Extend DTOs with:

```rust
MemoryUpdateRequest
MemoryPinRequest
MemoryArchiveRequest
MemoryForgetRequest
MemoryCompactRequest
MemoryImportRequest
MemoryImportResult
MemoryExportRequest
MemoryExportResult
MemoryContextExclusion
MemoryBatchLimits
```

Update `MemoryService` to match the contract and keep old CLI names as transport mapping only, not separate behavior.

- [ ] **Step 4: Implement lifecycle modules**

Implement update/reinforce/supersede/contradict/pin/archive/forget/review/compact/import/export by composing SQLite metadata updates, vector updates/deletes, graph mirror operations, redaction, and job events. `compact` creates a new memory, writes `derived_from` graph edges, and archives source memories only when `archive_sources=true`.

- [ ] **Step 5: Run lifecycle tests**

Run:

```bash
cargo test -p axon-memory lifecycle --no-fail-fast
cargo test -p axon-services memory --no-fail-fast
```

Expected: all memory operations pass through one service and DTO set.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-api/src crates/axon-memory/src crates/axon-services/src
git commit -m "feat: complete memory lifecycle service"
```

---

### Task 8: Memory Batch Boundaries And Partial-Failure Recovery — DONE

**Status (P1-V01, confirmed 2026-07-09 audit):** fully implemented —
`MemoryBatchLimits`/`crates/axon-memory/src/vector.rs` and the partial-failure
recovery path in `crates/axon-memory/src/vector/batch.rs` (returns one
outcome per record, in input order, so a caller can recover from a partial
batch failure) are landed. Checkboxes below were stale (unchecked despite
landed code) and are now checked.

**Files:**
- Modify: `crates/axon-memory/src/vector.rs`
- Modify: `crates/axon-memory/src/sqlite.rs`
- Modify: `crates/axon-memory/src/migration.rs`
- Modify: `crates/axon-memory/src/migrations/*`
- Modify: `crates/axon-jobs/src/workers/**/*.rs`
- Test: `crates/axon-memory/src/batch_tests.rs`
- Test: `crates/axon-memory/src/migration_tests.rs`

**Interfaces:**
- Consumes: `MemoryBatchLimits`, Qdrant scroll/page APIs, SQLite metadata indexes, graph mirror transactions.
- Produces: bounded embedding/upsert batches, Qdrant pagination, metadata indexes, graph transaction strategy, and recovery checkpoints.

- [x] **Step 1: Add failing batch/recovery tests**

```rust
#[tokio::test]
async fn memory_upsert_uses_configured_batch_size() {
    let service = memory_service_with_batch_limit(2).await;
    service.import(memories_fixture(5)).await.unwrap();
    assert_eq!(service.fake_vectors().upsert_batch_sizes(), vec![2, 2, 1]);
}

#[tokio::test]
async fn partial_vector_failure_keeps_memory_in_review_with_recovery_marker() {
    let service = memory_service_with_vector_failure_after(1).await;
    let result = service.import(memories_fixture(3)).await.unwrap_err();
    assert_eq!(result.code.to_string(), "memory.partial_failure");
    let review = service.review(MemoryReviewRequest::default()).await.unwrap();
    assert!(review.memories.iter().any(|m| m.status == MemoryStatus::Review));
}
```

- [x] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-memory batch partial --no-fail-fast`

Expected: failures for missing batch limits/recovery markers.

- [x] **Step 3: Add indexes and limits**

Add SQLite indexes for:

```sql
CREATE INDEX IF NOT EXISTS idx_memory_records_status_scope ON memory_records(status, scope_kind, scope_value);
CREATE INDEX IF NOT EXISTS idx_memory_records_type_status ON memory_records(memory_type, status);
CREATE INDEX IF NOT EXISTS idx_memory_records_updated ON memory_records(updated_at);
CREATE INDEX IF NOT EXISTS idx_memory_reinforcement_memory_time ON memory_reinforcement(memory_id, created_at);
```

Add `MemoryBatchLimits { embed_batch_size, upsert_batch_size, qdrant_page_size, graph_tx_batch_size }` with bounded defaults.

- [x] **Step 4: Implement partial-failure recovery**

On partial failure:

```text
SQLite row written but vector failed: status=review, history event memory.vector_failed, no recallable vector refs
Vector written but graph failed: status=active if graph optional, warning graph.write_failed, graph repair checkpoint
Forget/archive vector delete failed: status transition persists, cleanup debt records vector delete selector by memory_id
Import batch partial failure: successful memories remain durable; failed items return item-level ApiError entries
```

- [x] **Step 5: Run batch/recovery tests**

Run: `cargo test -p axon-memory batch migration partial --no-fail-fast`

Expected: bounded batch and partial recovery tests pass.

- [x] **Step 6: Commit**

Run:

```bash
git add crates/axon-memory/src crates/axon-jobs/src
git commit -m "feat: bound memory batches and recovery"
```

---

### Task 9: CLI, MCP, And REST Memory Contract — DONE

Completed via `docs/pipeline-unification/plans/2026-07-08-rest-memory-surface.md`
(split out per engineering review as independent of the job cutover). The REST
surface moved from the single opaque `POST /v1/memory` passthrough to per-verb
`/v1/memories` routes matching `surfaces/rest-contract.md`, each with
`AuthContext`/`CallerContext` extraction mirroring `sources.rs`'s pattern (the
old handler had none — a security review finding that drove the split). CLI,
MCP, and REST all now expose `import`/`export` end-to-end over the existing
`axon-memory` `MemoryStore` trait (previously unwired despite the store already
implementing both); the REST `import`/`export` routes carry an explicit 10 MiB
body-size limit.

**Deprecation, not removal:** the old `POST /v1/memory` route is kept
functional (RFC 8594 `Deprecation` header + `Link` to `/v1/memories`) rather
than deleted in the same change, since external clients (e.g. the desktop
palette app) may still depend on the old shape. Follow-up filed to remove it
once known clients have migrated: `axon_rust-69fq1` ("Remove deprecated
POST /v1/memory passthrough route").

**Files:**
- Modify: `crates/axon-cli/src/commands/memory.rs`
- Modify: `crates/axon-mcp/src/**/memory*.rs`
- Modify: `crates/axon-web/src/**/memory*.rs`
- Modify: `crates/axon-api/src/mcp_schema.rs`
- Modify: `apps/web/openapi/axon.json` only through the schema generator, not by hand
- Test: `crates/axon-cli/src/commands/memory_tests.rs`
- Test: `crates/axon-mcp/src/memory_tests.rs`
- Test: `crates/axon-web/src/memory_tests.rs`

**Interfaces:**
- Consumes: completed `MemoryService`.
- Produces: contract routes and actions from `runtime/memory-contract.md` and `surfaces/rest-contract.md`.

- [ ] **Step 1: Add failing transport parity tests**

Test that CLI, MCP, and REST map to the same DTO operation names:

```rust
#[test]
fn mcp_memory_registry_contains_contract_subactions() {
    let actions = memory_subactions();
    for expected in ["remember", "search", "context", "show", "link", "supersede", "reinforce", "contradict", "pin", "archive", "forget", "review", "compact"] {
        assert!(actions.contains(expected), "missing {expected}");
    }
}

#[tokio::test]
async fn rest_forget_uses_memory_service_and_returns_forgotten_status() {
    let app = test_app().await;
    let created = post_memory(&app, memory_body()).await;
    let deleted = delete_memory(&app, &created.memory_id).await;
    assert_eq!(deleted["status"], "forgotten");
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run:

```bash
cargo test -p axon-cli memory --no-fail-fast
cargo test -p axon-mcp memory --no-fail-fast
cargo test -p axon-web memory --no-fail-fast
```

Expected: missing operations fail.

- [ ] **Step 3: Wire transports to service DTOs**

Map operations:

```text
CLI: axon memory remember/search/context/show/link/supersede/reinforce/contradict/pin/archive/forget/review/compact/import/export
MCP: memory/<same subaction>
REST: /v1/memories routes from rest-contract.md
```

Do not duplicate lifecycle logic in transports. Transports parse, authorize, call service, redact response, and render.

- [ ] **Step 4: Apply response visibility rules**

Untrusted CLI JSON, MCP responses, and REST responses use the same redaction/visibility filtering. `forgotten` never returns body content. `archived`, `superseded`, `contradicted`, and `review` statuses follow Task 6 rules.

- [ ] **Step 5: Run transport tests**

Run:

```bash
cargo test -p axon-cli memory --no-fail-fast
cargo test -p axon-mcp memory --no-fail-fast
cargo test -p axon-web memory --no-fail-fast
```

Expected: memory surface parity tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-cli/src crates/axon-mcp/src crates/axon-web/src crates/axon-api/src
git commit -m "feat: expose complete memory contract"
```

---

### Task 10: Legacy Job Store Blockers And Reset Behavior

**Files:**
- Modify: `crates/axon-jobs/src/migrations.rs`
- Modify: `crates/axon-jobs/src/unified/ops.rs`
- Modify: `crates/axon-services/src/reset.rs`
- Modify: `crates/axon-web/src/**/reset*.rs`
- Test: `crates/axon-jobs/src/legacy_blocker_tests.rs`
- Test: `crates/axon-services/src/reset_tests.rs`

**Interfaces:**
- Consumes: full durable job model from Task 3A.
- Produces: `LegacyStoreAudit`, `LegacyStoreBlocker`, and admin-approved reset/import behavior.

- [ ] **Step 1: Add failing legacy blocker tests**

```rust
#[tokio::test]
async fn non_empty_legacy_job_tables_block_reset() {
    let pool = pool_with_legacy_crawl_job().await;
    let result = reset_all_stores(&pool, admin_snapshot()).await;
    let err = result.unwrap_err();
    assert_eq!(err.code.to_string(), "reset.legacy_store_non_empty");
}

#[tokio::test]
async fn empty_legacy_job_tables_do_not_block_reset() {
    let pool = pool_with_empty_legacy_tables().await;
    let result = reset_all_stores(&pool, admin_snapshot()).await;
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run tests to confirm current gap**

Run: `cargo test -p axon-jobs legacy blocker --no-fail-fast`

Expected: failure because old-store blockers/reset behavior is not complete.

- [ ] **Step 3: Implement legacy audit**

Detect known legacy tables for crawl/embed/extract/ingest/watch families. If any contain rows, return `ApiError` with:

```json
{
  "code": "reset.legacy_store_non_empty",
  "stage": "planning",
  "retryable": false,
  "severity": "failed",
  "visibility": "internal",
  "details": {
    "tables": ["crawl_jobs"],
    "actions": ["import_legacy_jobs", "archive_legacy_store", "admin_clear_legacy_store"]
  }
}
```

- [ ] **Step 4: Gate reset and destructive cleanup**

Reset requires `axon:admin`, typed auth snapshot, legacy audit pass, and explicit confirmation. The reset receipt records legacy table counts and chosen action.

- [ ] **Step 5: Run reset tests**

Run:

```bash
cargo test -p axon-jobs legacy --no-fail-fast
cargo test -p axon-services reset --no-fail-fast
```

Expected: legacy blockers and reset admin behavior pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/axon-jobs/src crates/axon-services/src crates/axon-web/src
git commit -m "feat: block unsafe resets with legacy job rows"
```

---

### Task 11: End-To-End Failure Guards

**Files:**
- Test: `crates/axon-jobs/src/security_error_memory_e2e_tests.rs`
- Test: `crates/axon-services/src/security_error_memory_e2e_tests.rs`
- Test: `crates/axon-mcp/src/security_error_memory_e2e_tests.rs`
- Test: `crates/axon-web/src/security_error_memory_e2e_tests.rs`

**Interfaces:**
- Consumes: Tasks 1-10.
- Produces: executable failure guards for auth snapshot enforcement, redaction failure, memory isolation, and provider cooling.

- [ ] **Step 1: Add e2e failure guard tests**

Add tests:

```rust
#[tokio::test]
async fn redaction_failure_blocks_memory_vector_write() {
    let service = memory_service_with_secret_detector_failure().await;
    let result = service.remember(memory("authorization: bearer abcdef0123456789abcdef")).await;
    assert_eq!(result.unwrap_err().code.to_string(), "redaction.failed");
    assert_eq!(service.fake_vectors().upsert_count(), 0);
}

#[tokio::test]
async fn normal_query_does_not_return_memory_without_intent() {
    let app = test_app_with_memory_and_documents().await;
    let query = post_query(&app, "phase 3b memory").await;
    assert!(query.results.iter().all(|hit| hit.vector_namespace != "memory"));
    let memory = post_memory_search(&app, "phase 3b memory").await;
    assert!(memory.results.iter().any(|hit| hit.vector_namespace == "memory"));
}

#[tokio::test]
async fn recovered_job_uses_original_auth_snapshot() {
    let app = test_app().await;
    let job = enqueue_local_job_without_local_scope(&app).await;
    mark_stale(&app, &job.job_id).await;
    let recovered = recover_job(&app, &job.job_id).await;
    assert_eq!(recovered.error.code, "auth.scope_required");
}
```

- [ ] **Step 2: Run e2e tests to confirm failures are meaningful**

Run:

```bash
cargo test -p axon-jobs security_error_memory_e2e --no-fail-fast
cargo test -p axon-services security_error_memory_e2e --no-fail-fast
```

Expected: failures point to any missed wiring from earlier tasks.

- [ ] **Step 3: Fix missed wiring only**

Fix missing enforcement points found by the tests. Do not add compatibility aliases or merge memory into source retrieval to make tests pass.

- [ ] **Step 4: Run required check set**

Run:

```bash
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-memory --no-fail-fast
```

Expected: all checks pass. If a command fails, record the exact failing test and fix before continuing.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/axon-jobs/src crates/axon-services/src crates/axon-mcp/src crates/axon-web/src crates/axon-memory/src
git commit -m "test: add security error memory failure guards"
```

---

## Self-Review Notes

- Auth scope requirements are covered in Tasks 1 and 2.
- Job auth snapshot propagation for watches, retries, stale reclaim, child jobs, prune, reset, local, execute, and memory jobs is covered in Task 2.
- ApiError propagation, provider cooling/retry fields, item-level errors, and redaction-failure handling are covered in Task 3.
- Shared redaction report and fail-closed behavior for public writes are covered in Task 4 and Task 11.
- Memory vector/graph/retrieval integration is covered in Tasks 5, 6, 7, 8, and 9.
- Memory remains distinct from source retrieval by namespace and explicit memory intent in Tasks 5 and 11.
- Old-store blockers/reset behavior is covered in Task 10.
- Suggested checks are included in Task 11.
