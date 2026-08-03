# Finish Job Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining open items in Task 3A (`2026-07-04-full-durable-job-cutover.md`, Tasks 6 and 8) by cutting `Crawl`/`Embed`/`Ingest` job execution over to the unified `JobStore`, mirroring the already-shipped `Extract` cutover — while fixing the two defects an engineering review found in that shipped precedent (an unconditional `trusted_system` auth snapshot, and zero callers of the worker-wake notify) before tripling their surface area, and fixing the unified worker's serial-only claim loop before deleting the legacy per-family lane concurrency it currently provides.

**Architecture:** `Extract` is already fully cut over (`crates/axon-services/src/extract.rs::extract_start_with_context` → `ExtractRunner` in `crates/axon-services/src/runtime/job_runners.rs` → `crates/axon-services/src/runtime/sqlite/extract_bridge.rs`). This plan repeats that pattern for `Crawl`, `Embed`, `Ingest`, one at a time, each fully verified before the next — but only after Task 0 fixes two cross-cutting gaps in the shared unified-worker infrastructure that every ported family would otherwise inherit silently: (1) `notify_unified()` has zero callers anywhere, so every job enqueued today waits a full poll interval instead of waking immediately; (2) the unified worker's claim loop runs each job inline/serially (`lanes = 1`, confirmed by its own log line), while the legacy `embed_worker`/`ingest_worker` it will replace run `embed_lanes`/`ingest_lanes`-way parallel (up to 32/16). Both gaps are pre-existing in the shipped Extract cutover — low-impact there because of Extract's low volume — but would become real regressions the moment Crawl/Embed/Ingest (the system's higher-volume job kinds) move onto the same infrastructure. This plan also corrects the shipped `extract_start_with_context`'s auth-snapshot handling to match the pattern the rest of the codebase's newer `*_source_job.rs` files already use (`Option<AuthSnapshot>` + `.unwrap_or_else(|| AuthSnapshot::trusted_system(...))`), rather than inventing a new pattern.

Crawl has one apparent wrinkle Extract didn't — `crates/axon-jobs/src/workers.rs`'s existing comment claims crawl futures are `!Send`, requiring a dedicated single-lane worker. Evidence against this claim is strong (crawl already runs inside a plain `tokio::spawn`, which requires `Send`, today), so Task 2 leads with a compile-check to settle it empirically before choosing an implementation strategy, rather than assuming the comment is correct.

**Split out of this plan, tracked separately (see the "Related Plans" section below):** provider-cooling wiring, redaction-boundary extension to CLI/artifacts/traces, and the REST memory route split — all architecturally independent of the job cutover, per engineering review. Reset's legacy-store confirmation gap stays in this plan (Task 5) since it's small, cheap, and unrelated in *code* but related in *theme* (another "don't silently trust legacy state" fix), and a security review found its originally-proposed fix (a `config.toml`-persistable flag) needed correcting anyway.

**Tech Stack:** Rust 2024, Tokio, `sqlx` SQLite/WAL, `axon-api::source` job DTOs, `axon-jobs`, `axon-services`, `axon-cli`, `axon-mcp`, `axon-web`.

## Global Constraints

- Do not migrate or backfill legacy `axon_crawl_jobs`/`axon_embed_jobs`/`axon_ingest_jobs` rows into the unified tables. In-flight legacy jobs finish on the legacy path; new jobs enqueue on the unified path from the cutover commit forward.
- Every job-backed operation still returns a `JobDescriptor`/`JobStartOutcome`; do not change the CLI/MCP/REST-facing shape of `crawl`/`embed`/`ingest` responses, only what backs them.
- Preserve panic guard, cancellation, heartbeat, stale reclaim, and retry semantics through the cutover.
- **Never construct `AuthSnapshot::trusted_system(...)` unconditionally in a function that has (or could have) a real caller identity available.** Accept `Option<AuthSnapshot>`, fall back to `trusted_system` only when genuinely absent (internal/system-triggered calls like watch/refresh), matching `crates/axon-services/src/web_source/web_source_job.rs`'s established pattern.
- `axon:write` does not imply `axon:admin`, `axon:execute`, or `axon:local`.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.
- Follow the repo's sidecar test-file convention (`foo.rs` + `foo_tests.rs` declared via `#[path]`) and the ≤500-line monolith policy.

---

## Source-Of-Truth Contracts

- `docs/pipeline-unification/plans/2026-07-04-full-durable-job-cutover.md` (Tasks 6 and 8)
- `docs/pipeline-unification/runtime/job-contract.md`
- `docs/pipeline-unification/runtime/auth-contract.md`
- `docs/pipeline-unification/delivery/cutover-contract.md`

## Related Plans (split out, do not implement here)

- `2026-07-08-provider-cooling.md` — Phase 3B Task 3 gap
- `2026-07-08-redaction-boundary-extension.md` — Phase 3B Task 4 gap
- `2026-07-08-rest-memory-surface.md` — Phase 3B Task 9 gap

## Current-State Anchors

- Proven cutover template: `crates/axon-services/src/extract.rs::extract_start_with_context`, `crates/axon-services/src/runtime/job_runners.rs::{build_registry, ExtractRunner}`, `crates/axon-services/src/runtime/sqlite/extract_bridge.rs`, `crates/axon-services/src/runtime/sqlite.rs` (the `if kind == JobKind::Extract { ... }` bridge dispatch).
- Established real auth-snapshot pattern to copy (NOT `extract.rs`'s unconditional `trusted_system`): `crates/axon-services/src/web_source/web_source_job.rs` — `input.auth_snapshot.clone().unwrap_or_else(|| AuthSnapshot::trusted_system("runtime"))`. Caller-context construction at the transport layer: `crates/axon-web/src/server/handlers/sources.rs::caller_context_from_auth(&AuthContext) -> CallerContext`, then `AuthSnapshot::from_caller(&caller, Visibility::Internal, policy_version)`.
- Worker-wake gap: `crates/axon-jobs/src/workers.rs::notify_unified()` (zero callers anywhere). Poll fallback: `crates/axon-jobs/src/workers/unified.rs`'s `POLL_INTERVAL` in the `tokio::select!` at the top of `unified_worker_loop`.
- Concurrency gap: `crates/axon-jobs/src/workers/unified.rs::unified_worker_loop`'s inner `while processed < WORKER_BATCH_LIMIT` loop calls `run_unified_claimed(...).await` inline before claiming the next job — fully serial. `crates/axon-jobs/src/workers/spawn_unified.rs` logs `lanes = 1` explicitly. Compare against legacy `embed_lanes`/`ingest_lanes`-driven `tokio::spawn` loops in `crates/axon-jobs/src/workers.rs::spawn_workers`.
- Legacy dual-run spawn point: `crates/axon-jobs/src/workers.rs::spawn_workers` — spawns `crawl_worker` (single lane), `embed_worker` (multi-lane), `ingest_worker` (multi-lane) alongside `spawn_unified_worker`.
- Legacy enqueue call sites: `crates/axon-services/src/crawl.rs::crawl_start_with_context`, `crates/axon-services/src/embed.rs::embed_start_with_context`, `crates/axon-services/src/ingest.rs::ingest_start_with_context` — all currently call `service_context.jobs.enqueue(JobPayload::{Crawl,Embed,Ingest} { .. })`.
- Legacy active-runtime table readers: `crates/axon-jobs/src/backend.rs`, `crates/axon-jobs/src/query.rs`.
- Crawl's disputed `!Send` claim: comment at `crates/axon-jobs/src/workers.rs` near the `crawl_worker` spawn. Contradicting evidence: `crates/axon-jobs/src/workers/runners/crawl.rs::run_crawl_job` already executes inside a plain `tokio::spawn` today.
- Reset legacy-store handling: `crates/axon-services/src/reset.rs::execute_prepared_reset`, `crates/axon-services/src/reset/execution.rs` (wipes unconditionally today, writes an audit receipt after the fact — a deliberate but under-visible tradeoff, not an oversight).

## File Structure

- Modify: `crates/axon-jobs/src/workers/unified.rs` (bounded concurrent claim-and-run)
- Modify: `crates/axon-core/src/config/types/config.rs` (add `unified_worker_concurrency`)
- Modify: `crates/axon-services/src/extract.rs` (fix `trusted_system` → `Option<AuthSnapshot>`, add `notify_unified()` call)
- Modify: `crates/axon-services/src/jobs.rs` (same fix, second unconditional `trusted_system` site)
- Create: `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs`, `embed_bridge.rs`, `ingest_bridge.rs`
- Modify: `crates/axon-services/src/runtime/job_runners.rs` (add `CrawlRunner`, `EmbedRunner`, `IngestRunner`)
- Modify: `crates/axon-services/src/crawl.rs`, `embed.rs`, `ingest.rs` (`*_start_with_context` → unified enqueue, real caller auth, notify)
- Modify: `crates/axon-services/src/runtime/sqlite.rs` (bridge dispatch for `JobKind::{Crawl,Embed,Ingest}`)
- Modify: `crates/axon-jobs/src/workers.rs` (remove legacy worker spawns once each family is ported)
- Modify: `crates/axon-jobs/src/backend.rs`, `crates/axon-jobs/src/query.rs` (remove legacy table mappings)
- Modify: `crates/axon-api/src/reset.rs`, `crates/axon-services/src/reset.rs`, `reset/execution.rs`, `reset/sqlite.rs` (legacy-store confirmation, CLI-flag-only)
- Modify: `crates/axon-cli/src/**` (auth context threading into `*_start_with_context` call sites; `--confirm-legacy-wipe` CLI flag)
- Create: `crates/axon-jobs/src/security_error_memory_e2e_tests.rs` additions (the jobs-crate slice only — MCP/web slices live in the split-out redaction/memory plans)

---

## Task 0: Fix The Shared Unified-Worker Infrastructure Before Porting Any Family

**Files:**
- Modify: `crates/axon-jobs/src/workers/unified.rs`
- Modify: `crates/axon-core/src/config/types/config.rs`, `crates/axon-core/src/config/types/config_impls.rs`
- Modify: `crates/axon-services/src/extract.rs`
- Modify: `crates/axon-services/src/jobs.rs`
- Test: `crates/axon-jobs/src/unified_tests.rs`
- Test: `crates/axon-services/src/extract_tests.rs`

**Interfaces:**
- Consumes: `tokio::sync::Semaphore`, existing `JobRunnerRegistry`/`run_unified_claimed`.
- Produces: `unified_worker_loop` claiming and running jobs concurrently up to a bounded limit; `notify_unified()` actually called on enqueue; `extract_start_with_context`/`crates/axon-services/src/jobs.rs`'s job-tracking helper both stop constructing `AuthSnapshot::trusted_system` unconditionally.

This task exists because an engineering review found these two gaps in the *already-shipped* Extract cutover, and every subsequent task in this plan would otherwise silently inherit and triple them.

- [x] **Step 1: Write a failing prompt-wakeup latency test**

Add to `crates/axon-jobs/src/unified_tests.rs`:

```rust
#[tokio::test]
async fn enqueued_job_is_claimed_within_one_wakeup_not_a_full_poll_interval() {
    let (pool, notify, shutdown) = unified_test_harness().await;
    let handle = tokio::spawn(crate::workers::unified::unified_worker_loop(
        Arc::clone(&pool),
        Arc::clone(&notify),
        shutdown.clone(),
        None,
    ));
    let store = crate::unified::SqliteUnifiedJobStore::new((*pool).clone());
    let started = std::time::Instant::now();
    let job = store.create(job_request_fixture("memory")).await.unwrap();
    notify.notify_one();
    let claimed_at = wait_for_status_change(&store, job.job_id, LifecycleStatus::Queued, std::time::Duration::from_millis(500))
        .await
        .expect("job should leave Queued well before a full poll interval");
    assert!(
        claimed_at.elapsed_since(started) < std::time::Duration::from_secs(1),
        "job took a full poll interval to be claimed — notify_unified() is not being called"
    );
    shutdown.cancel();
    let _ = handle.await;
}
```

Add `unified_test_harness()` and `wait_for_status_change()` helpers next to whatever this file's existing unified-store test fixtures already use (reuse, don't duplicate — `unified_tests.rs` already has fixtures like `job_request_fixture` per the Task 3A cutover).

- [x] **Step 2: Write a failing concurrency test**

Add:

```rust
#[tokio::test]
async fn unified_worker_claims_and_runs_multiple_jobs_concurrently() {
    let (pool, notify, shutdown) = unified_test_harness().await;
    let concurrency_marker = Arc::new(tokio::sync::Semaphore::new(0));
    let registry = registry_with_slow_concurrent_runner(Arc::clone(&concurrency_marker));
    let handle = tokio::spawn(crate::workers::unified::unified_worker_loop(
        Arc::clone(&pool),
        Arc::clone(&notify),
        shutdown.clone(),
        Some(registry),
    ));
    let store = crate::unified::SqliteUnifiedJobStore::new((*pool).clone());
    for _ in 0..4 {
        store.create(job_request_fixture("memory")).await.unwrap();
    }
    notify.notify_one();
    // The slow runner blocks until it observes >1 concurrently in-flight,
    // proving the worker didn't serialize them.
    let observed_concurrent = wait_for_concurrent_marker(&concurrency_marker, std::time::Duration::from_secs(2)).await;
    assert!(observed_concurrent >= 2, "expected at least 2 jobs running concurrently, saw {observed_concurrent}");
    shutdown.cancel();
    let _ = handle.await;
}
```

Add `registry_with_slow_concurrent_runner`/`wait_for_concurrent_marker` as small local test helpers — a fake `UnifiedJobRunner` whose `run()` increments a shared counter, awaits a short sleep, decrements, and returns `Ok(())`.

- [x] **Step 3: Run tests and confirm failure**

Run: `cargo test -p axon-jobs enqueued_job_is_claimed_within_one_wakeup unified_worker_claims_and_runs_multiple_jobs_concurrently --no-fail-fast`

Expected: both FAIL — the first because nothing calls `notify.notify_one()` from the production enqueue path (only the test does, proving the mechanism works when invoked, but nothing in `extract_start_with_context` invokes it), the second because `unified_worker_loop` awaits each claimed job inline.

- [x] **Step 4: Add `unified_worker_concurrency` config**

Add `unified_worker_concurrency: usize` to `Config` (default `8` — a conservative middle ground between Extract's implicit `1` and legacy `embed_lanes`' ceiling of `32`; document in the field's doc comment that this is deliberately not auto-derived from `embed_lanes`/`ingest_lanes` because after Task 4 those fields stop being consumed for job execution and become dead config, which a later cleanup pass should remove). Wire it into `Config::default()`/`test_default()` and `~/.axon/config.toml` parsing per this repo's "Adding fields to `Config`" convention.

- [x] **Step 5: Bound the claim loop with a semaphore**

In `crates/axon-jobs/src/workers/unified.rs::unified_worker_loop`, replace the inline `run_unified_claimed(&pool, &claimed, &shutdown, registry.as_deref()).await;` with a semaphore-permitted spawn:

```rust
let semaphore = Arc::new(tokio::sync::Semaphore::new(unified_worker_concurrency));
// ... inside the claim loop:
match claim_next_unified_job_unchecked(&pool).await {
    Ok(Some(claimed)) => {
        let permit = Arc::clone(&semaphore).acquire_owned().await.expect("semaphore not closed");
        let pool = Arc::clone(&pool);
        let shutdown = shutdown.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            run_unified_claimed(&pool, &claimed, &shutdown, registry.as_deref()).await;
            drop(permit);
        });
        processed += 1;
    }
    Ok(None) => break,
    Err(error) => { /* unchanged */ }
}
```

Thread `unified_worker_concurrency: usize` as a new parameter into `unified_worker_loop` and `spawn_unified_worker` (`crates/axon-jobs/src/workers/spawn_unified.rs`), sourced from `cfg.unified_worker_concurrency` at the `spawn_workers` call site. Update the `spawn_unified_worker` log line's hardcoded `lanes = 1` to log the real configured concurrency instead — it is actively misleading once this fix lands.

- [x] **Step 6: Wire `notify_unified()` into the enqueue path**

`notify_unified()` (`crates/axon-jobs/src/workers.rs`) needs to be reachable from `axon-services`. Check whether `ServiceContext`/`WorkerHandles` already exposes a callable path (search for how `crawl_notify`/`embed_notify` are reached from `axon-services` today, since the legacy family enqueue functions already call their own family notify successfully — mirror that exact wiring for the unified notify handle instead of inventing a new plumbing path). Add `service_context.notify_unified()` (or the equivalent real method name found) as the last line of `extract_start_with_context`, right after `store.create(...)` succeeds.

- [x] **Step 7: Fix `extract_start_with_context`'s unconditional `trusted_system`**

Change `extract_start_with_context`'s signature to accept `caller: Option<&AuthSnapshot>` (or thread through however `web_source_job.rs`'s `input.auth_snapshot: Option<AuthSnapshot>` field is populated — read that file's full struct definition first to match the established shape exactly, do not invent a divergent parameter shape). Replace:

```rust
auth_snapshot: AuthSnapshot::trusted_system("runtime"),
```

with:

```rust
auth_snapshot: caller
    .cloned()
    .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
```

Update `extract_start_with_context`'s three real callers (CLI `crates/axon-cli/src/commands/extract.rs`, MCP `crates/axon-mcp/src/server/handlers_extract.rs`, REST `crates/axon-web/src/server/handlers/{rest/,}async_jobs.rs` or wherever the real extract-start REST handler lives) to construct a real `CallerContext`/`AuthSnapshot` the same way `crates/axon-web/src/server/handlers/sources.rs::caller_context_from_auth` already does for the newer source pipeline, and pass it through. Where a transport genuinely has no authenticated caller available (e.g. an internal scheduler trigger), pass `None` explicitly rather than silently defaulting — the call site should make the "no real caller" case visible in the diff, not implicit.

- [x] **Step 8: Fix the second unconditional `trusted_system` site**

`crates/axon-services/src/jobs.rs` (line ~201 per the review) has a second unconditional `AuthSnapshot::trusted_system("runtime")` construction in a job-tracking helper. Read its surrounding function to determine whether it has caller context available; apply the same `Option<AuthSnapshot>` fix if it does, or leave it as `trusted_system` with a `// no caller identity available at this call site — see docs/pipeline-unification/runtime/auth-contract.md` comment explaining why if it is genuinely an internal-only path (e.g. system-triggered watch/reclaim), so a future reader doesn't mistake it for an oversight.

- [x] **Step 9: Run tests**

```bash
cargo test -p axon-jobs unified --no-fail-fast
cargo test -p axon-services extract --no-fail-fast
```

Expected: PASS, including both new tests from Steps 1-2.

- [x] **Step 10: Commit**

```bash
git add crates/axon-jobs/src crates/axon-core/src/config crates/axon-services/src/extract.rs crates/axon-services/src/jobs.rs crates/axon-cli/src crates/axon-mcp/src crates/axon-web/src
git commit -m "fix(jobs): bound unified worker concurrency, wire enqueue notify, fix unconditional trusted_system auth"
```

## Task 1: Port Embed (Simplest — No `!Send` Wrinkle)

**Files:**
- Create: `crates/axon-services/src/runtime/sqlite/embed_bridge.rs`
- Modify: `crates/axon-services/src/runtime/job_runners.rs`
- Modify: `crates/axon-services/src/embed.rs`
- Modify: `crates/axon-services/src/runtime/sqlite.rs`
- Test: `crates/axon-services/src/embed_tests.rs`
- Test: `crates/axon-services/src/runtime/job_runners_tests.rs`

**Interfaces:**
- Consumes: `axon_jobs::boundary::JobStore::create`, `axon_api::source::{JobCreateRequest, JobKind as UnifiedJobKind, JobIntent, JobPriority, JobStagePlan, PipelinePhase, AuthSnapshot, MetadataMap}`, `crate::runtime::job_runners::{UnifiedJobRunner, UnifiedClaimedJob, SqliteUnifiedJobStore}`, Task 0's fixed `notify_unified()` wiring and real-caller-auth pattern.
- Produces: `embed_start_with_context` enqueueing via the unified store with real caller auth and immediate wakeup; `EmbedRunner` executing embed jobs. Tasks 2-3 reuse this exact pattern.

- [x] **Step 1: Read the exact current embed enqueue/execute code**

Read `crates/axon-services/src/embed.rs` in full, and whichever function `crates/axon-jobs/src/workers/runners/embed.rs`'s `embed_worker` calls to run a real embed job — that function's name/signature is what `EmbedRunner::run` must call.

- [x] **Step 2: Write failing test for unified enqueue with real auth and wakeup**

Add to `crates/axon-services/src/embed_tests.rs`:

```rust
#[tokio::test]
async fn embed_start_with_context_enqueues_on_unified_job_store_with_caller_auth() {
    let ctx = crate::testing::service_context_with_unified_jobs().await;
    let cfg = axon_core::config::Config::test_default();
    let caller = AuthSnapshot::from_caller(
        &CallerContext {
            actor: Some("user_1".to_string()),
            transport: TransportKind::Cli,
            scopes: vec![AuthScope::Read, AuthScope::Write],
            visibility_ceiling: Visibility::Internal,
        },
        Visibility::Internal,
        "test",
    );
    let outcome = embed_start_with_context(&cfg, "https://example.com", &ctx, None, None, Some(&caller))
        .await
        .expect("embed_start_with_context should enqueue");
    let store = ctx.job_store().expect("unified job store must be attached");
    let job = store
        .get(axon_api::source::JobId(
            uuid::Uuid::parse_str(&outcome.result.job_id).unwrap(),
        ))
        .await
        .unwrap()
        .expect("job row must exist");
    assert_eq!(job.job_kind, axon_api::source::JobKind::Embed);
    assert_eq!(job.auth_snapshot.caller_id.as_deref(), Some("user_1"));
    assert_ne!(job.auth_snapshot.granted_scopes, vec![AuthScope::Admin]);
}
```

If `crate::testing::service_context_with_unified_jobs` does not exist yet, reuse whatever helper `extract_tests.rs`'s own unified-store fixture already uses — do not write a second one.

- [x] **Step 3: Run test and confirm failure**

Run: `cargo test -p axon-services embed_start_with_context_enqueues_on_unified_job_store_with_caller_auth --no-fail-fast`

Expected: FAIL — `outcome` still comes from `JobPayload::Embed` on the legacy backend.

- [x] **Step 4: Implement `embed_start_with_context` unified enqueue**

Add a `caller: Option<&AuthSnapshot>` parameter (matching Task 0 Step 7's `extract_start_with_context` shape exactly) and replace the legacy enqueue body:

```rust
pub async fn embed_start_with_context(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    _source_type: Option<&str>,
    caller: Option<&AuthSnapshot>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    let _ = tx;
    let config_json = config_snapshot_json(cfg)?;
    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: UnifiedJobKind::Embed,
            job_intent: JobIntent::Run,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![JobStagePlan {
                phase: PipelinePhase::Embedding,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: None,
            }],
            request: Some(serde_json::json!({
                "input": input,
                "config_json": config_json,
            })),
            auth_snapshot: caller
                .cloned()
                .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("embed_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e.message.into() })?;
    service_context.notify_unified();
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_embed_start_result(descriptor.job_id.0.to_string()),
    })
}
```

Update every existing caller of `embed_start_with_context` (CLI/MCP/REST) to pass a real `Some(&caller_snapshot)` constructed the same way Task 0 Step 7 wired Extract's callers — do not leave them passing `None` out of laziness when a real caller identity is available at that call site.

- [x] **Step 5: Implement `EmbedRunner`**

In `crates/axon-services/src/runtime/job_runners.rs`, mirror `ExtractRunner`'s heartbeat/cancellation/error-wrapping shape:

```rust
struct EmbedRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for EmbedRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Embedding).await;
        if shutdown.is_cancelled() {
            return Err(embed_error("embed canceled before running"));
        }
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| embed_error("embed job has no request payload"))?;
        let input = request
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| embed_error("embed job request is missing `input`"))?
            .to_string();
        let config_json = request
            .get("config_json")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let effective_cfg = apply_config_snapshot(&self.cfg, config_json).map_err(|error| {
            ApiError::new(
                "job_runner.invalid_config_snapshot",
                ErrorStage::Planning,
                error.to_string(),
            )
        })?;
        let embed_fut = crate::embed::embed_sync(&effective_cfg, &input);
        tokio::select! {
            _ = shutdown.cancelled() => Err(embed_error("embed canceled")),
            result = embed_fut => result.map(|_summary| ()).map_err(|error| embed_error(error.to_string())),
        }
    }
}

fn embed_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.embed_failed",
        ErrorStage::Embedding,
        message.into(),
    )
}
```

Replace `crate::embed::embed_sync` with the real function name found in Step 1 if it differs. Register it in `build_registry`.

- [x] **Step 6: Add the embed bridge**

Create `crates/axon-services/src/runtime/sqlite/embed_bridge.rs` by copying `extract_bridge.rs`, adapting the request-payload field extraction to Embed's `{"input": "...", "config_json": "..."}` shape.

- [x] **Step 7: Wire the bridge dispatch**

In `crates/axon-services/src/runtime/sqlite.rs`, add `mod embed_bridge;` and `if kind == JobKind::Embed { ... }` branches mirroring the existing `JobKind::Extract` branches.

- [x] **Step 8: Run the enqueue test and confirm pass**

Run: `cargo test -p axon-services embed_start_with_context_enqueues_on_unified_job_store_with_caller_auth --no-fail-fast`

Expected: PASS.

- [x] **Step 9: Add and run an end-to-end embed execution test with a wakeup-latency assertion**

Add to `crates/axon-services/src/embed_tests.rs`:

```rust
#[tokio::test]
async fn embed_job_runs_end_to_end_and_is_claimed_promptly() {
    let ctx = crate::testing::service_context_with_unified_jobs_and_workers().await;
    let cfg = axon_core::config::Config::test_default();
    let started = std::time::Instant::now();
    let outcome = embed_start_with_context(&cfg, "https://example.com", &ctx, None, None, None)
        .await
        .unwrap();
    let job_id = uuid::Uuid::parse_str(&outcome.result.job_id).unwrap();
    let job = crate::testing::wait_for_terminal_status(&ctx, job_id, std::time::Duration::from_secs(5))
        .await
        .expect("job should reach a terminal status");
    assert!(matches!(
        job.status,
        axon_api::source::LifecycleStatus::Completed
            | axon_api::source::LifecycleStatus::CompletedDegraded
    ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "embed job took longer than a poll-interval-free path should — notify_unified() regression?"
    );
}
```

Reuse `extract_tests.rs`'s equivalent helpers if named differently.

Run:

```bash
cargo test -p axon-services embed --no-fail-fast
```

Expected: PASS.

- [x] **Step 10: Commit**

```bash
git add crates/axon-services/src/embed.rs crates/axon-services/src/embed_tests.rs crates/axon-services/src/runtime/job_runners.rs crates/axon-services/src/runtime/job_runners_tests.rs crates/axon-services/src/runtime/sqlite.rs crates/axon-services/src/runtime/sqlite/embed_bridge.rs crates/axon-cli/src crates/axon-mcp/src crates/axon-web/src
git commit -m "feat(jobs): cut embed over to the unified job store"
```

## Task 2: Resolve The Crawl `!Send` Question, Then Port Crawl

**Files:**
- Modify: `crates/axon-services/src/runtime/job_runners.rs`
- Modify: `crates/axon-services/src/crawl.rs`
- Modify: `crates/axon-services/src/runtime/sqlite.rs`
- Create: `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs`
- Test: `crates/axon-services/src/crawl_tests.rs`

**Interfaces:**
- Consumes: same unified-store primitives as Task 1.
- Produces: `crawl_start_with_context` on the unified store; `CrawlRunner` executing crawl jobs.

The `!Send` claim behind crawl's dedicated single-lane legacy worker is very likely stale: `run_crawl_job` already executes inside a plain `tokio::spawn` today (which requires `Send`), and neither `axon-crawl/src/engine.rs` nor the crawl job runner contains `Rc`/`RefCell`/`LocalSet`/`spawn_local` — the fingerprints of genuine `!Send` state held across an `.await`. **Default to the plain `ExtractRunner`-shaped implementation (Step 2 below). Only fall back to the thread-isolation design (Step 3) if Step 1's compile check fails with a concrete named `Send` violation** — do not write or use the thread-isolation code preemptively.

- [x] **Step 1: Prove or disprove `!Send` with a real compile check**

Write the plain-shaped `CrawlRunner` from Step 2 below and run:

```bash
cargo check -p axon-services
```

If it compiles cleanly, the `!Send` claim is false — proceed with Step 2 as written and skip Step 3 entirely. If it fails with a `Send` bound error, read the error to identify the exact type that isn't `Send`, then use Step 3, scoping the thread-isolation boundary as tightly as possible around only that type's usage (not the whole crawl engine) if feasible.

- [x] **Step 2: Default path — plain `CrawlRunner` (use unless Step 1 proves otherwise)**

```rust
struct CrawlRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for CrawlRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Fetching).await;
        if shutdown.is_cancelled() {
            return Err(crawl_error("crawl canceled before running"));
        }
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| crawl_error("crawl job has no request payload"))?;
        let urls: Vec<String> = request
            .get("urls")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| crawl_error("crawl job request is missing a `urls` array"))?;
        let config_json = request
            .get("config_json")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let effective_cfg = apply_config_snapshot(&self.cfg, config_json).map_err(|error| {
            ApiError::new(
                "job_runner.invalid_config_snapshot",
                ErrorStage::Planning,
                error.to_string(),
            )
        })?;
        let crawl_fut = crate::crawl::run_crawl_for_unified_job(&effective_cfg, &urls);
        tokio::select! {
            _ = shutdown.cancelled() => Err(crawl_error("crawl canceled")),
            result = crawl_fut => result.map(|_summary| ()).map_err(|error| crawl_error(error.to_string())),
        }
    }
}

fn crawl_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.crawl_failed",
        ErrorStage::Fetching,
        message.into(),
    )
}
```

Implement `crate::crawl::run_crawl_for_unified_job(cfg, urls) -> Result<Summary, Box<dyn Error>>` as a thin wrapper around whatever function the legacy `crawl_worker` (in `crates/axon-jobs/src/workers/runners/crawl.rs`) actually calls to execute a crawl — found by reading that file in Step 1.

- [x] **Step 3: Fallback path — dedicated-thread isolation (ONLY if Step 1's compile check genuinely fails)**

If and only if Step 1 fails with a real `Send` error, isolate just the offending type behind a dedicated OS thread with a single-threaded runtime, racing the result against `shutdown` (not the unconditional `rx.await` an earlier draft of this plan used — that version had no way to observe cancellation):

```rust
struct CrawlRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for CrawlRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Fetching).await;
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| crawl_error("crawl job has no request payload"))?
            .clone();
        let cfg = Arc::clone(&self.cfg);
        let thread_shutdown = shutdown.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let join_handle = std::thread::spawn(move || {
            let local = tokio::task::LocalSet::new();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build crawl worker runtime");
            let result = rt.block_on(local.run_until(async move {
                run_crawl_request(&cfg, request, thread_shutdown).await
            }));
            let _ = tx.send(result);
        });
        tokio::select! {
            _ = shutdown.cancelled() => {
                // The thread observes the cloned `shutdown` token internally
                // (run_crawl_request must poll it between batches) and is
                // expected to exit; join it with a bounded grace period so
                // we don't leak the OS thread even if it's slow to notice.
                let _ = tokio::task::spawn_blocking(move || join_handle.join()).await;
                Err(crawl_error("crawl canceled"))
            }
            result = rx => result
                .map_err(|_| crawl_error("crawl worker thread panicked or dropped"))?
                .map_err(|error| crawl_error(error.to_string())),
        }
    }
}
```

`run_crawl_request(cfg, request_json, shutdown) -> Result<(), ApiError>` must poll `shutdown.is_cancelled()` between crawl page batches (not just once at the top) so the join in the `shutdown.cancelled()` branch above actually completes promptly instead of blocking for the full crawl duration.

- [x] **Step 4: Write failing enqueue test**

Add to `crates/axon-services/src/crawl_tests.rs`, mirroring Task 1 Step 2's shape (real caller auth, `JobKind::Crawl`).

- [x] **Step 5: Run test and confirm failure**

Run: `cargo test -p axon-services crawl_start_with_context_enqueues_on_unified_job_store_with_caller_auth --no-fail-fast`

Expected: FAIL.

- [x] **Step 6: Implement `crawl_start_with_context` unified enqueue**

Same pattern as Task 1 Step 4: add `caller: Option<&AuthSnapshot>`, `job_kind: UnifiedJobKind::Crawl`, `PipelinePhase::Fetching`, request payload `{"urls": urls, "config_json": config_json}` (keep the existing `apply_crawl_defaults(cfg)` call), call `service_context.notify_unified()` after enqueue.

- [x] **Step 7: Register `CrawlRunner`, add bridge, wire dispatch**

Same as Task 1 Steps 6-7, for `Crawl`.

- [x] **Step 8: Run enqueue and end-to-end tests**

```bash
cargo test -p axon-services crawl --no-fail-fast
```

Expected: PASS, including an end-to-end test mirroring Task 1 Step 9.

- [x] **Step 9: Commit**

```bash
git add crates/axon-services/src/crawl.rs crates/axon-services/src/crawl_tests.rs crates/axon-services/src/runtime/job_runners.rs crates/axon-services/src/runtime/sqlite.rs crates/axon-services/src/runtime/sqlite/crawl_bridge.rs
git commit -m "feat(jobs): cut crawl over to the unified job store"
```

## Task 3: Port Ingest

**Files:**
- Modify: `crates/axon-services/src/runtime/job_runners.rs`
- Modify: `crates/axon-services/src/ingest.rs`
- Modify: `crates/axon-services/src/runtime/sqlite.rs`
- Create: `crates/axon-services/src/runtime/sqlite/ingest_bridge.rs`
- Test: `crates/axon-services/src/ingest_tests.rs`

**Interfaces:**
- Consumes: same unified-store primitives as Task 1.
- Produces: `ingest_start_with_context` on the unified store; `IngestRunner` executing sessions/prepared-sessions ingest (the only `IngestSource` variants that still execute after the Phase 12 axon-ingest shrink — every other variant already returns a clean error at execution time per `crates/axon-jobs/src/workers/runners/ingest.rs::execute_ingest_source`).

- [x] **Step 1: Read the current ingest enqueue/execute code**

Read `crates/axon-services/src/ingest.rs::ingest_start_with_context` and `crates/axon-jobs/src/workers/runners/ingest.rs::{run_ingest_job, execute_ingest_source, execute_prepared_sessions_ingest}` in full.

- [x] **Step 2: Write failing enqueue test**

Add to `crates/axon-services/src/ingest_tests.rs`, mirroring Task 1 Step 2's shape, `job_kind: UnifiedJobKind::Ingest`, request payload `{"source": serde_json::to_value(&source)?}`.

- [x] **Step 3: Run test and confirm failure**

Run: `cargo test -p axon-services ingest_start_with_context_enqueues_sessions_source_on_unified_job_store_with_caller_auth --no-fail-fast`

Expected: FAIL.

- [x] **Step 4: Implement `ingest_start_with_context` unified enqueue**

Add `caller: Option<&AuthSnapshot>`, `job_kind: UnifiedJobKind::Ingest`, `PipelinePhase::Parsing`, keep the existing `preflight_ingest_source(cfg, &source).await?;` call, call `service_context.notify_unified()` after enqueue.

- [x] **Step 5: Implement `IngestRunner`**

Mirror `ExtractRunner`, deserializing the full `IngestSource` from `claimed.request_json["source"]` and calling the same dispatch `execute_ingest_source`/`execute_prepared_sessions_ingest` already implement.

- [x] **Step 6: Register `IngestRunner`, add bridge, wire dispatch**

Same as Task 1 Steps 6-7, for `Ingest`.

- [x] **Step 7: Run enqueue and end-to-end tests**

```bash
cargo test -p axon-services ingest --no-fail-fast
```

Expected: PASS.

- [x] **Step 8: Commit**

```bash
git add crates/axon-services/src/ingest.rs crates/axon-services/src/ingest_tests.rs crates/axon-services/src/runtime/job_runners.rs crates/axon-services/src/runtime/sqlite.rs crates/axon-services/src/runtime/sqlite/ingest_bridge.rs
git commit -m "feat(jobs): cut ingest over to the unified job store"
```

## Task 4: Retire The Legacy Per-Family Workers And Table Readers

**Files:**
- Modify: `crates/axon-jobs/src/workers.rs`
- Modify: `crates/axon-jobs/src/backend.rs`
- Modify: `crates/axon-jobs/src/query.rs`
- Delete: `crates/axon-jobs/src/workers/runners/crawl.rs`, `embed.rs`, `ingest.rs` (and their `_tests.rs` sidecars) once nothing references them
- Test: `crates/axon-jobs/src/legacy_removal_tests.rs`

**Interfaces:**
- Consumes: Tasks 0-3.
- Produces: no active-runtime reference to `axon_crawl_jobs`/`axon_embed_jobs`/`axon_ingest_jobs`.

- [x] **Step 1: Add the legacy-reference blocker test**

```rust
#[test]
fn active_runtime_no_longer_names_legacy_family_tables() {
    let source = include_str!("backend.rs").to_string()
        + include_str!("query.rs")
        + include_str!("workers.rs");
    for table in ["axon_crawl_jobs", "axon_embed_jobs", "axon_ingest_jobs"] {
        assert!(
            !source.contains(table),
            "legacy table {table} still referenced in active runtime"
        );
    }
}
```

- [x] **Step 2: Run test and confirm failure**

Run: `cargo test -p axon-jobs active_runtime_no_longer_names_legacy_family_tables --no-fail-fast`

Expected: FAIL.

- [x] **Step 3: Remove the legacy worker spawns, resolving the reclaim question explicitly**

In `spawn_workers`, delete the `crawl_worker`/`embed_worker`/`ingest_worker` spawn blocks and their `Notify` construction. **Decision, not a footnote: keep the watchdog's reclaim sweep wired to a lightweight legacy-table scan** (matching how `extract_notify` was kept wired into the watchdog's generic reclaim sweep during the Extract cutover) so any in-flight legacy row that existed at the moment of this deploy still gets a terminal status eventually, rather than stranding permanently. Do not add new legacy-row execution — only reclaim-to-failed, consistent with the "no migration/backfill" global constraint.

- [x] **Step 4: Remove legacy family table mappings**

In `crates/axon-jobs/src/backend.rs`, remove the `JobKind::{Crawl,Embed,Ingest}` → table-name mapping arms. In `crates/axon-jobs/src/query.rs`, remove the list/count/cleanup/clear query functions targeting those three tables.

- [x] **Step 5: Fix fallout**

Run `cargo check --workspace --all-targets` and fix every resulting compile error by routing through the unified bridges from Tasks 1-3.

- [x] **Step 6: Run the blocker test and full job-crate suite**

```bash
cargo test -p axon-jobs active_runtime_no_longer_names_legacy_family_tables --no-fail-fast
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services --no-fail-fast
```

Expected: PASS.

- [x] **Step 7: Verify with the durable-job-cutover plan's own final check**

```bash
rg -n "axon_crawl_jobs|axon_embed_jobs|axon_extract_jobs|axon_ingest_jobs|JobKind::Crawl|JobKind::Embed|JobKind::Extract|JobKind::Ingest" crates/axon-jobs crates/axon-services crates/axon-cli crates/axon-mcp crates/axon-web
```

Expected: matches only in migration modules and bridge modules (which legitimately still route on `JobKind::Crawl`/etc. to select which bridge to call).

- [x] **Step 8: Commit**

```bash
git add crates/axon-jobs/src crates/axon-services/src
git commit -m "refactor(jobs): remove legacy crawl/embed/ingest job runtime"
```

## Task 5: Fix Reset's Legacy-Store Confirmation Gap (CLI-Flag-Only)

**Files:**
- Modify: `crates/axon-api/src/reset.rs`
- Modify: `crates/axon-services/src/reset.rs`
- Modify: `crates/axon-services/src/reset/execution.rs`
- Modify: `crates/axon-cli/src/commands/reset.rs` (or wherever reset's clap args live)
- Test: `crates/axon-services/src/reset_tests.rs`

**Interfaces:**
- Consumes: `axon_jobs::unified::{detect_incompatible_legacy_jobs, LegacyJobStoreBlocker}` (already exists).
- Produces: `ResetPlan.blockers` genuinely populated; `reset()` refuses to execute when legacy rows are present unless a **per-invocation CLI flag** (not a persistable config field) confirms it.

Today's behavior (`crates/axon-services/src/reset/execution.rs`) is a deliberate design choice — treating `axon reset --yes` as the "explicit admin reset receipt" the security contract requires — with a real visibility gap: `ResetPlan.blockers`/`ResetResult.blockers` are hardcoded to `Vec::new()`, so a caller inspecting the dry-run plan can't see legacy rows exist before they wipe them. A security review of an earlier draft of this task found the originally-proposed fix — a `reset_confirm_legacy_wipe: bool` field on `Config`, settable via `config.toml` — could be set once and permanently defeat the "distinct explicit confirmation" the task is trying to add. **This must be a CLI-flag-only value, explicitly rejected if it arrives via config file or environment variable**, so it can never become a standing footgun.

- [x] **Step 1: Write failing dry-run visibility test**

```rust
#[tokio::test]
async fn dry_run_plan_surfaces_non_empty_legacy_job_tables_as_a_blocker() {
    let cfg = test_config_with_legacy_crawl_job_row().await;
    let result = reset(&cfg).await.unwrap();
    assert!(result.dry_run);
    assert!(
        result.blockers.iter().any(|b| b.contains("axon_crawl_jobs")),
        "expected a legacy-store blocker naming axon_crawl_jobs, got {:?}",
        result.blockers
    );
}
```

- [x] **Step 2: Write failing config-source rejection test**

```rust
#[tokio::test]
async fn legacy_wipe_confirmation_sourced_from_config_file_is_rejected() {
    let mut cfg = test_config_with_legacy_crawl_job_row().await;
    cfg.yes = true;
    cfg.reset_confirm_legacy_wipe = true;
    cfg.reset_confirm_legacy_wipe_source = ConfigValueSource::TomlFile;
    let err = reset(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("--confirm-legacy-wipe must be passed as a CLI flag"),
        "config-sourced confirmation must be rejected, got: {err}"
    );
}

#[tokio::test]
async fn legacy_wipe_confirmation_from_cli_flag_wipes_and_records_receipt() {
    let mut cfg = test_config_with_legacy_crawl_job_row().await;
    cfg.yes = true;
    cfg.reset_confirm_legacy_wipe = true;
    cfg.reset_confirm_legacy_wipe_source = ConfigValueSource::CliFlag;
    let result = reset(&cfg).await.unwrap();
    assert!(!result.dry_run);
    assert!(result.audit_events.iter().any(|e| e.contains("legacy")));
}
```

Add `test_config_with_legacy_crawl_job_row` as a helper mirroring however other reset tests in this file already build a scratch DB.

- [x] **Step 3: Run tests and confirm failure**

Run: `cargo test -p axon-services reset --no-fail-fast`

Expected: FAIL.

- [x] **Step 4: Add the confirmation flag with a source-tracking field**

Add `reset_confirm_legacy_wipe: bool` (default `false`) and `reset_confirm_legacy_wipe_source: ConfigValueSource` (an enum `{ CliFlag, TomlFile, EnvVar, Unset }`, default `Unset`) to `Config`. Wire the CLI flag `--confirm-legacy-wipe` to set both `reset_confirm_legacy_wipe = true` and `reset_confirm_legacy_wipe_source = ConfigValueSource::CliFlag`. If this repo's config-parsing layer would otherwise also accept it from `config.toml`/env (check `crates/axon-core/src/config/parse/`), explicitly exclude `reset_confirm_legacy_wipe` from that parsing path — it must only ever be settable by the CLI flag.

- [x] **Step 5: Populate `blockers` and gate execution on CLI-sourced confirmation**

In `prepare_reset`, when `wants_any_sqlite(&stores)`, call `sqlite::detect_legacy_jobs(&cfg.sqlite_path).await` and push a human-readable string into `ResetPlan.blockers`/`ResetResult.blockers` if it returns `Some`. Before `execute_prepared_reset` mutates anything, if a legacy blocker was detected and `!(cfg.reset_confirm_legacy_wipe && cfg.reset_confirm_legacy_wipe_source == ConfigValueSource::CliFlag)`, return an error whose message says `"--confirm-legacy-wipe must be passed as a CLI flag"` when the source is wrong, or names the table and required flag when it's simply missing.

- [x] **Step 6: Record the legacy wipe explicitly in the audit trail**

In `reset/execution.rs`, when a legacy blocker was present and confirmed, push a `"reset.legacy_store_wiped"` entry into `audit_events` alongside the existing `record_legacy_reset_receipt` call.

- [x] **Step 7: Run reset tests**

```bash
cargo test -p axon-services reset --no-fail-fast
```

Expected: PASS.

- [x] **Step 8: Commit**

```bash
git add crates/axon-core/src/config crates/axon-services/src/reset.rs crates/axon-services/src/reset/execution.rs crates/axon-services/src/reset_tests.rs crates/axon-cli/src
git commit -m "fix(reset): require a CLI-flag-only confirmation before wiping non-empty legacy job tables"
```

## Task 6: Full Verification And Plan Closeout

**Files:**
- Modify: this plan file (check off completed tasks)
- Modify: `docs/pipeline-unification/plans/2026-07-04-full-durable-job-cutover.md` (mark Tasks 6 and 8 done with evidence)

**Interfaces:**
- Consumes: Tasks 0-5.
- Produces: closeout evidence.

- [x] **Step 1: Run the durable-job-cutover plan's own Task 9 verification**

```bash
cargo test -p axon-api source_job --no-fail-fast
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services jobs --no-fail-fast
cargo test -p axon-cli jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
rg -n "axon_crawl_jobs|axon_embed_jobs|axon_extract_jobs|axon_ingest_jobs|JobKind::Crawl|JobKind::Embed|JobKind::Extract|JobKind::Ingest" crates/axon-jobs crates/axon-services crates/axon-cli crates/axon-mcp crates/axon-web
```

Expected: PASS; `rg` matches only in migration/bridge modules.

- [x] **Step 2: Verify the concurrency and latency fixes hold under the full suite**

```bash
cargo test -p axon-jobs enqueued_job_is_claimed_within_one_wakeup_not_a_full_poll_interval unified_worker_claims_and_runs_multiple_jobs_concurrently --no-fail-fast
cargo test -p axon-services embed_job_runs_end_to_end_and_is_claimed_promptly --no-fail-fast
```

Expected: PASS.

- [x] **Step 3: Full workspace gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace --no-fail-fast
cargo xtask check-layering
cargo xtask check-repo-structure
```

Expected: PASS.

- [x] **Step 4: Update the source plan doc**

In `2026-07-04-full-durable-job-cutover.md`, check off Task 6 and Task 8's step boxes and add an evidence note pointing at this plan's commits.

- [x] **Step 5: Commit**

```bash
git add docs/pipeline-unification/plans
git commit -m "docs(pipeline): close out full durable job cutover Tasks 6 and 8"
```

## Closeout Evidence (2026-07-09)

Commits on `finish-job-cutover-impl`, in order: `c353774cf` (Task 0),
`97124ca14` (Task 1: embed), `46575ef6a` (Task 2: crawl), `4c2effea4`
(Task 3: ingest), `ca7ea71d1` (Task 4: retire legacy workers — see scope
adjustment below), `79384d0d9` (Task 5: reset legacy-store confirmation),
`a500ae416` (env-matrix drift fix surfaced by Task 6 Step 3).

**Task 4 scope adjustment (documented in `ca7ea71d1`'s commit message):**
An audit run before touching `backend.rs`/`query.rs` found the plan's Step 4
instruction ("remove the `JobKind::{Crawl,Embed,Ingest}` table-name mapping
arms") would have broken live functionality the plan did not account for:
- `crates/axon-jobs/src/watch/dispatch.rs::enqueue_change_crawl` (and its
  in-flight guard `crawl_job_active`) still constructs the legacy
  `JobPayload::Crawl` directly and writes to `axon_crawl_jobs` — reachable
  from `axon watch exec`, `POST /v1/watch/{id}/run`, and the automatic watch
  scheduler.
- `crates/axon-services/src/refresh.rs`'s `latest_crawl_config_json`/
  `latest_ingest_config_json` still read the legacy tables for `axon refresh`.
- `SqliteServiceRuntime::count_jobs`/`count_jobs_by_status` in
  `crates/axon-services/src/runtime/sqlite.rs` were never bridged to the
  unified store for Extract/Embed/Crawl/Ingest (every other method was),
  so `axon status`, the queue-summary logger, and the starvation watchdog
  would have silently under-reported once legacy rows stopped accumulating.

Task 4 was executed as: retire the legacy in-process worker lanes
(`crawl_worker`/`embed_worker`/`ingest_worker` and their now-orphaned
support modules), keep the legacy `backend.rs`/`query.rs`/table
infrastructure in place for the still-live watch/refresh call sites above,
and bridge `count_jobs`/`count_jobs_by_status` to close the metrics gap.
This satisfies Task 4's actual intent (no in-process execution runs against
legacy family tables anymore) without breaking `watch`/`refresh`, which are
out of this plan's scope to re-port.

**Step 1 `rg` verification:** 316 non-test matches remain across
`crates/axon-jobs`/`crates/axon-services`/`crates/axon-cli`/`crates/axon-mcp`/
`crates/axon-web`, composed entirely of: SQL migration files, bridge modules
(`*_bridge.rs`, `*_runner.rs` — legitimately dispatch on `JobKind` to select
the unified-store bridge), the legacy `backend.rs`/`query.rs`/`store.rs`/
`ops/*` infrastructure retained per the scope adjustment above, and CLI/MCP/
web surface code rendering the transport-neutral `ServiceJob` shape (agnostic
of unified-vs-legacy backing by design).

**Step 3 full-suite result:** `cargo test --workspace --no-fail-fast` — all
crates green except 3 pre-existing failures confirmed unrelated to this
plan (verified by reproducing identically with this branch's changes
stashed out): `setup_split_help_snapshots_match`/
`all_command_help_filters_inherited_global_noise` (CLI help snapshot drift
from `preflight`, last touched in `697e7f2e4` before this branch existed),
`services_up_starts_only_infrastructure_services` (Docker-compose-state-
dependent test, unrelated to job cutover code), and
`schemas::tests::rest_schema_registry_matches_current_openapi_route_inventory`
(missing `/v1/prune/exec`/`/v1/prune/plan` OpenAPI routes — unrelated prune
work, not touched by this plan). `env_config_boundary_matrix_is_current`
initially failed (two undocumented env keys, one introduced by this branch's
Task 0) and was fixed in `a500ae416`.

## Self-Review

- Spec coverage: durable-job-cutover Task 6 → Tasks 1-3; Task 8 → Task 4.
- Engineering review findings applied: unconditional `trusted_system` auth fixed at its source (Task 0, both call sites) before being tripled by Tasks 1-3, not left as a copy-pasted regression; `notify_unified()` wired everywhere it was missing; unified worker concurrency bounded before Task 4 deletes the lane-based alternative; Task 2 restructured so the (likely unnecessary) thread-isolation design is a gated fallback, not the default; Task 4's legacy-reclaim question resolved as an explicit decision; Task 5's confirmation flag hardened to reject config/env sourcing.
- Split out per review (architecturally independent, tracked separately): provider cooling, redaction boundary extension, REST memory surface — see `docs/pipeline-unification/plans/2026-07-08-provider-cooling.md`, `2026-07-08-redaction-boundary-extension.md`, `2026-07-08-rest-memory-surface.md`.
- Placeholder scan: every step names exact files and function/type names cited from direct code reads, or explicitly instructs a read-first step where the exact current name could not be verified directly (crawl's real execution entry point, the `jobs.rs` `trusted_system` site's caller-availability, the config-parsing exclusion list) — these are read-first steps, not deferred-decision placeholders.
- Type consistency: `JobKind`/`UnifiedJobKind`, `JobCreateRequest`, `JobStagePlan`, `PipelinePhase`, `AuthSnapshot`, `MetadataMap`, `UnifiedJobRunner`, `UnifiedClaimedJob`, `SqliteUnifiedJobStore` used identically across Tasks 1-3, matching the real `ExtractRunner`/`extract_bridge.rs`/`web_source_job.rs` precedents.
