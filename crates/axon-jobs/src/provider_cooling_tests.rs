//! Provider cooling: bound + wire `ProviderCooling` into unified job claim
//! eligibility.
//!
//! # Task 1 findings — real API surface (recorded before any code was written)
//!
//! - `axon_error::cooling::ProviderCooling` (`crates/axon-error/src/cooling.rs`):
//!   `pub struct ProviderCooling { provider_id: Option<String>, cooldown_until:
//!   DateTime<Utc>, reason: Option<String> }` with constructors
//!   `ProviderCooling::new(cooldown_until: DateTime<Utc>) -> Self`,
//!   `.with_provider(impl Into<String>) -> Self`, `.with_reason(impl
//!   Into<String>) -> Self`. All real, exactly as the plan's illustrative
//!   samples assumed.
//! - `axon_error::ApiError` (`crates/axon-error/src/api_error.rs`):
//!   `with_provider_cooling(self, cooling: ProviderCooling) -> Self` sets
//!   `provider_id`/`cooldown_until`/`details["cooling_reason"]` and forces
//!   `retryable = true`. `provider_cooling(&self) -> Option<ProviderCooling>`
//!   reconstructs one from `cooldown_until`/`provider_id` (drops `reason`).
//!   Both real, exactly as the plan assumed.
//! - **Claim query** — real name confirmed: `claim_next_unified_job_unchecked`
//!   in `crates/axon-jobs/src/workers/unified.rs`. Exact SQL:
//!   `SELECT job_id, kind, attempt, request_json, auth_snapshot_json FROM jobs
//!   WHERE status IN ('queued', 'waiting', 'blocked') ORDER BY <priority CASE>,
//!   updated_at ASC, job_id ASC LIMIT 1`. This is the ONLY unified claim
//!   query (there is a second, unrelated legacy per-family claim function,
//!   `ops::lifecycle::claim_next_pending_for_attempt`, for the
//!   `axon_crawl_jobs`/`axon_embed_jobs`/etc. tables — out of scope here).
//! - **Existing index**: `idx_axon_jobs_claim` in migration
//!   `crates/axon-jobs/src/migrations/0019_unified_jobs_contract_fields.sql`,
//!   exactly as the plan named it: `ON jobs(status, <priority CASE>,
//!   updated_at ASC, job_id ASC) WHERE status IN ('queued', 'waiting',
//!   'blocked')`.
//! - **Timestamp representation**: `jobs.updated_at`/`started_at`/
//!   `finished_at` are TEXT (RFC3339), via the `Timestamp(pub String)`
//!   newtype (`crates/axon-api/src/source/ids.rs`) — NOT epoch integers. The
//!   new `cooldown_until` column follows the same TEXT/RFC3339 convention for
//!   consistency with every other timestamp column on `jobs`.
//! - **Deviation from the plan's guessed status-update hook**: the plan
//!   guessed a function it called "`transition_to_waiting_with_cooling`" in
//!   `crates/axon-jobs/src/unified/control.rs`. `control.rs` is real but only
//!   owns `cancel_job`/`retry_job`/`recover_jobs`/`cleanup_jobs`/
//!   `list_job_artifacts`/`reset_jobs`/`store_capabilities` — there is no
//!   generic status-transition function there. The REAL generic status-write
//!   function is `SqliteUnifiedJobStore::update_job_status` in
//!   `crates/axon-jobs/src/unified/ops.rs` (exposed via the `JobStore` trait
//!   as `update_status(JobStatusUpdate)`), which already handles the
//!   `Running -> Waiting` transition (allowed by
//!   `crate::state_machine::validate_transition`) and already writes
//!   `last_error_json` from `status.error: Option<SourceError>`. However
//!   `JobStatusUpdate`/`SourceError` are both DTOs with ~15 real call sites
//!   using full struct literals across `axon-services`, and neither carries a
//!   `cooldown_until` field — extending them would be a wide, risky DTO
//!   change out of this plan's stated file scope. Cooling is instead applied
//!   via a small additive method, `SqliteUnifiedJobStore::apply_provider_cooling`,
//!   that callers invoke alongside a `Waiting` `update_status` call.
//!   `update_job_status` itself unconditionally clears `cooldown_until` to
//!   NULL whenever the target status is not `Waiting`, satisfying "clear on
//!   every transition to a non-Waiting status" for every caller of
//!   `update_status` (not just the cooling caller).
//! - **Terminal-status write path**: the plan guessed `mark_terminal` in
//!   `crates/axon-jobs/src/workers/unified.rs` — real and confirmed (writes
//!   `status`/`phase`/`last_error_json` for `job_attempts`/`job_stages` on
//!   Completed/CompletedDegraded/Failed/Canceled). It does NOT touch
//!   `cooldown_until` and does not need to: `update_job_status` (the function
//!   this plan wires cooling into) already clears the column on every
//!   non-Waiting write, and `mark_terminal` writes directly to `jobs`/
//!   `job_attempts`/`job_stages` via raw SQL rather than going through
//!   `update_job_status` — so it independently needs (and gets) the same
//!   clear-on-terminal `UPDATE ... SET cooldown_until = NULL` added to its own
//!   `jobs` UPDATE statement.
//! - **Real provider-saturation call site**: searched
//!   `crates/axon-services`, `crates/axon-vector` (TEI), and the
//!   `UnifiedJobRunner` registry (`crates/axon-services/src/runtime/
//!   job_runners.rs`, which registers `ProviderProbe`/`Extract`/`Memory`
//!   runners only). Finding: TEI's 429/5xx retry-exhaustion path
//!   (`crates/axon-vector/src/ops/tei/tei_client.rs::send_chunk_with_retries`)
//!   returns a plain `Box<dyn Error>` string, not an `ApiError`, and is only
//!   reachable today through the LEGACY per-family embed job runner
//!   (`crates/axon-jobs/src/workers/runners/embed.rs`, writing to
//!   `axon_embed_jobs`) — a different job system from the unified `jobs`
//!   table this plan's claim query targets. No unified `UnifiedJobRunner`
//!   currently calls TEI or constructs an `ApiError::with_provider_cooling`.
//!   There is therefore no live call site to "wire" without inventing one.
//!   This test file instead proves the generic mechanism end-to-end
//!   (clamping, claim exclusion, clear-on-terminal) against the real store,
//!   which is the reusable piece any future TEI-calling `UnifiedJobRunner`
//!   will plug into.

use std::time::Duration;

use super::*;
use axon_api::source::*;
use axon_error::cooling::ProviderCooling;

// This sidecar is declared at the crate root (`lib.rs`), not nested under a
// single module file, so `use super::*` only brings top-level `pub mod`
// names into scope (e.g. `boundary`, `store`) — it does not flatten their
// contents. `JobStore`/`open_sqlite_pool`/etc. are re-imported explicitly
// below because they live in those submodules.
use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use crate::unified::SqliteUnifiedJobStore;
use crate::workers::unified::claim_next_unified_job;

async fn store() -> SqliteUnifiedJobStore {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    seed_source(&pool).await;
    SqliteUnifiedJobStore::new(pool)
}

async fn seed_source(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO sources (
            source_id, committed_generation, summary_json, created_at, updated_at
        ) VALUES ('src_local', NULL, '{}', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .expect("seed source row");
}

fn create_request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_local".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: Some(SourceId::new("src_local")),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: Some(serde_json::json!({"source": "/tmp/project"})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    }
}

async fn run_to_waiting(store: &SqliteUnifiedJobStore, job_id: JobId) {
    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("queued -> running");
    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Waiting,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: Some("provider cooling".to_string()),
            error: None,
        })
        .await
        .expect("running -> waiting");
}

async fn stored_cooldown_until(store: &SqliteUnifiedJobStore, job_id: JobId) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT cooldown_until FROM jobs WHERE job_id = ?")
        .bind(job_id.0.to_string())
        .fetch_one(store.pool_for_tests())
        .await
        .expect("select cooldown_until")
}

#[tokio::test]
async fn cooldown_until_column_and_index_exist() {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
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

#[tokio::test]
async fn cooldown_until_is_clamped_to_a_maximum_window() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let far_future = chrono::Utc::now() + chrono::Duration::days(365);
    let cooling = ProviderCooling::new(far_future).with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");

    let raw = stored_cooldown_until(&store, job.job_id)
        .await
        .expect("cooldown_until set");
    let cooldown_until: chrono::DateTime<chrono::Utc> = raw.parse().expect("parse cooldown_until");
    assert!(
        cooldown_until
            <= chrono::Utc::now() + chrono::Duration::hours(1) + chrono::Duration::minutes(1),
        "cooldown_until must be clamped to the configured maximum window, got {cooldown_until:?}"
    );
}

#[tokio::test]
async fn cooling_job_is_not_claimable_before_cooldown_expires() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooldown_until = chrono::Utc::now() + chrono::Duration::seconds(30);
    let cooling = ProviderCooling::new(cooldown_until).with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");

    let claimed = claim_next_unified_job(store.pool_for_tests())
        .await
        .expect("claim query");
    assert!(
        claimed.is_none(),
        "a job in its cooling window must not be claimable"
    );
}

#[tokio::test]
async fn cooling_job_is_claimable_once_cooldown_expires() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    // Already-past cooldown: apply_provider_cooling still clamps to <= now +
    // MAX, but a value already in the past must not be pushed into the
    // future by clamping — clamping is a ceiling, not a floor.
    let cooldown_until = chrono::Utc::now() - chrono::Duration::seconds(1);
    let cooling = ProviderCooling::new(cooldown_until).with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");

    let claimed = claim_next_unified_job(store.pool_for_tests())
        .await
        .expect("claim query")
        .expect("expired cooldown job should be claimable");
    assert_eq!(claimed.job_id, job.job_id);

    let cooldown_after = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_after.is_none(),
        "cooldown_until must be cleared by the claim UPDATE"
    );
}

#[tokio::test]
async fn cooldown_until_is_cleared_on_completion() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");
    assert!(
        stored_cooldown_until(&store, job.job_id).await.is_some(),
        "precondition: cooldown_until should be set before completion"
    );

    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("waiting -> running");

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until.is_none(),
        "cooldown_until must be cleared once the job leaves Waiting"
    );
}

#[tokio::test]
async fn cooldown_until_is_cleared_by_mark_terminal_failure_path() {
    // mark_terminal (crates/axon-jobs/src/workers/unified.rs) writes directly
    // to `jobs` via raw SQL rather than going through update_job_status, so it
    // independently must clear cooldown_until on every terminal write. Cover
    // it via the real claim -> fail path exposed for tests.
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");

    // Precondition check: cooldown_until is set while the job sits in
    // Waiting. mark_terminal (exercised below via the test-only wrapper)
    // writes directly to `jobs` rather than through update_job_status, so it
    // needs its own independent clear — verify that clear fires from a job
    // that is still Waiting with an active cooldown, which is the real
    // shape a job in this state would be in when a worker's runner fails it.
    assert!(
        stored_cooldown_until(&store, job.job_id).await.is_some(),
        "precondition: cooldown_until should be set before terminal failure"
    );

    crate::workers::unified::mark_job_failed_for_tests(store.pool_for_tests(), job.job_id)
        .await
        .expect("mark_terminal failed path");

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until.is_none(),
        "cooldown_until must be cleared by the terminal write path too"
    );
}

#[tokio::test]
async fn apply_provider_cooling_does_not_push_a_past_deadline_into_the_future() {
    // Clamping is `min(requested, now + MAX)` — it must never raise an
    // already-earlier deadline. Guards against an off-by-max_fn direction bug.
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let five_minutes_ago = chrono::Utc::now() - chrono::Duration::minutes(5);
    let cooling = ProviderCooling::new(five_minutes_ago).with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");

    let raw = stored_cooldown_until(&store, job.job_id)
        .await
        .expect("cooldown_until set");
    let stored: chrono::DateTime<chrono::Utc> = raw.parse().expect("parse cooldown_until");
    let delta = (stored - five_minutes_ago).num_seconds().abs();
    assert!(
        delta <= 2,
        "cooldown_until should round-trip the original past deadline, got {stored:?} vs {five_minutes_ago:?}"
    );
}

#[tokio::test]
async fn apply_provider_cooling_only_applies_to_waiting_jobs() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    // Still Queued — never transitioned to Waiting.
    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    let result = store.apply_provider_cooling(job.job_id, cooling).await;
    assert!(
        result.is_err(),
        "cooling should only apply to a job that is in Waiting status"
    );
}

/// Sanity bound so a future change to the clamp constant is caught here
/// rather than only discovered by a DoS review.
#[test]
fn max_cooldown_window_is_bounded_to_one_hour() {
    assert_eq!(
        crate::unified::MAX_PROVIDER_COOLDOWN_WINDOW,
        Duration::from_secs(60 * 60)
    );
}

#[tokio::test]
async fn cooldown_until_is_cleared_by_cancel_job() {
    // cancel_job (crates/axon-jobs/src/unified/control.rs) writes directly to
    // `jobs` via raw SQL and permits Waiting -> Canceling/Canceled, so it
    // independently must clear cooldown_until on that transition too.
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");
    assert!(
        stored_cooldown_until(&store, job.job_id).await.is_some(),
        "precondition: cooldown_until should be set before cancel"
    );

    store
        .cancel(
            job.job_id,
            JobCancelRequest {
                reason: None,
                force_after_ms: None,
            },
        )
        .await
        .expect("cancel waiting job");

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until.is_none(),
        "cooldown_until must be cleared by cancel_job"
    );
}

#[tokio::test]
async fn cooldown_until_is_cleared_by_heartbeat_failure() {
    // record_heartbeat/update_heartbeat_summary (crates/axon-jobs/src/unified/
    // heartbeat.rs) writes directly to `jobs` via raw SQL and permits
    // Waiting -> Failed/Expired, so it independently must clear
    // cooldown_until on that transition too.
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");
    assert!(
        stored_cooldown_until(&store, job.job_id).await.is_some(),
        "precondition: cooldown_until should be set before heartbeat failure"
    );

    store
        .heartbeat(JobHeartbeat {
            job_id: job.job_id,
            attempt: 1,
            worker_id: None,
            phase: PipelinePhase::Complete,
            status: LifecycleStatus::Failed,
            stage_id: None,
            heartbeat_at: Timestamp::from(chrono::Utc::now()),
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        })
        .await
        .expect("waiting -> failed heartbeat");

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until.is_none(),
        "cooldown_until must be cleared by a terminal heartbeat write"
    );
}

#[tokio::test]
async fn cooldown_until_is_cleared_by_recovery_reset() {
    // reset_stale_job_for_recovery (crates/axon-jobs/src/unified/
    // control_helpers.rs) recycles a stale Waiting job back to queued via raw
    // SQL, so it independently must clear cooldown_until too.
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    run_to_waiting(&store, job.job_id).await;

    let cooling = ProviderCooling::new(chrono::Utc::now() + chrono::Duration::minutes(1))
        .with_provider("tei");
    store
        .apply_provider_cooling(job.job_id, cooling)
        .await
        .expect("apply cooling");
    assert!(
        stored_cooldown_until(&store, job.job_id).await.is_some(),
        "precondition: cooldown_until should be set before recovery reset"
    );

    let result = store
        .recover(JobRecoveryRequest {
            kind: None,
            stale_before: None,
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: true,
        })
        .await
        .expect("recover stale waiting job");
    assert_eq!(
        result.jobs_requeued, 1,
        "expected the waiting job to be requeued by recovery"
    );

    let cooldown_until = stored_cooldown_until(&store, job.job_id).await;
    assert!(
        cooldown_until.is_none(),
        "cooldown_until must be cleared by recovery's retry-reset path"
    );
}
