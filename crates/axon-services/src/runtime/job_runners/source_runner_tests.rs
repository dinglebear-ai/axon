use super::*;
use crate::runtime::job_runners::build_registry;
use axon_api::source::{
    AuthScope, AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent,
    JobKind as UnifiedJobKind, JobPriority, JobStagePlan, MetadataMap, PipelinePhase,
};
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;

async fn test_cfg() -> (tempfile::TempDir, Arc<Config>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    // Explicitly unset — asserts these tests exercise the no-data-plane
    // degraded path deterministically, independent of any ambient
    // QDRANT_URL/TEI_URL env the process happens to inherit.
    cfg.qdrant_url = String::new();
    cfg.tei_url = String::new();
    (tmp, Arc::new(cfg))
}

fn source_job_request(source_request: &SourceRequest) -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req-source-runner-test".to_string()),
        job_kind: UnifiedJobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Fetching,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        request: Some(serde_json::json!({
            "source_request": source_request,
        })),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_source_runner_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Build a claimed job + enqueue-only store for direct `SourceRunner::run`
/// invocation.
///
/// Tests in this file call `SourceRunner::run` directly rather than going
/// through `SqliteJobBackend::new_with_workers_and_registry` +
/// `build_registry`. `build_registry` opens its own `SqliteMemoryStore`
/// (rusqlite, synchronous `ensure_schema`) against the same fresh sqlite file
/// *before* the async `SqliteJobBackend` construction gets a chance to run
/// the composed sqlx migration set — this pre-existing ordering (present on
/// `main`, independent of this runner) makes the composed `memory/0003`
/// migration hit "duplicate column name: visibility" and fails backend
/// construction outright for every job kind's `new_with_workers_and_registry`
/// test in this crate, not just `Source`'s. Driving `SourceRunner::run`
/// directly against a plain enqueue-only `SqliteJobBackend::new` (no
/// `build_registry` involved) sidesteps that unrelated construction bug and
/// still exercises the runner's real logic end-to-end: real payload
/// deserialization, a real lazily-built `ServiceContext` (via
/// `crate::runtime::resolve_runtime`), and a real
/// `crate::source::index_source_with_auth` call.
async fn claim_source_job(
    store: &SqliteUnifiedJobStore,
    request_payload: serde_json::Value,
) -> UnifiedClaimedJob {
    let mut create_request = source_job_request(&SourceRequest::new("placeholder"));
    create_request.request = Some(request_payload.clone());
    let created = store
        .create(create_request)
        .await
        .expect("create source job row");
    UnifiedClaimedJob {
        job_id: created.job_id,
        kind: UnifiedJobKind::Source,
        attempt: 1,
        request_json: Some(request_payload),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
    }
}

/// (a) A claimed `Source` job actually runs the pipeline and reaches a
/// terminal status instead of hanging `running`/`queued` forever — the core
/// C4-02 / bead `axon_rust-mijoc` regression this runner fixes. With no
/// Qdrant/TEI configured, `index_source_with_auth` degrades cleanly to a
/// `Failed` `SourceResult` (data plane unconfigured) rather than erroring the
/// call outright, so this also proves the runner correctly propagates that
/// `Ok(SourceResult{status: Failed, ..})` into a real `Err(ApiError)` (which
/// the unified worker loop then turns into a job-store `Failed` transition)
/// rather than mis-reporting it as `Ok(())`/`Completed`.
#[tokio::test]
async fn source_runner_reaches_terminal_failed_without_data_plane() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::new("https://example.com/docs/getting-started");
    let claimed = claim_source_job(&store, serde_json::json!({"source_request": request})).await;

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err(
        "source job with no data plane configured must return Err, not Ok, so the unified \
         worker marks it Failed instead of Completed",
    );
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("data plane"),
        "expected the data_plane_unconfigured warning surfaced as the runner's ApiError, got: {}",
        error.message
    );
}

/// (b) A malformed/unroutable source request fails the job with a
/// descriptive `ApiError` (distinct failure mode from the data-plane
/// degradation above — this one never reaches `index_source_with_auth`'s
/// dispatch at all, it fails in the runner's own payload validation).
#[tokio::test]
async fn source_runner_with_missing_payload_fails_with_api_error() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let claimed = claim_source_job(&store, serde_json::json!({"not_a_source_request": true})).await;

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err("missing `source_request` payload must fail the job");
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("source_request"),
        "expected a validation error naming the missing field, got: {}",
        error.message
    );
}

#[tokio::test]
async fn source_runner_rechecks_local_scope_from_persisted_snapshot() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::local_path("/tmp/axon-missing-runner-local-auth-test", true);
    let mut claimed =
        claim_source_job(&store, serde_json::json!({"source_request": request})).await;
    claimed.auth_snapshot = AuthSnapshot::default();
    claimed.auth_snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err("worker-local source without local scope must fail the job");
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("axon:local"),
        "expected source runner to surface the service auth recheck, got: {}",
        error.message
    );
}

/// (c) Cancellation before the runner starts executing is honored: the
/// runner's own `shutdown.is_cancelled()` guard (mirroring every other
/// unified runner — Crawl/Embed/Extract/Ingest) short-circuits before doing
/// any work, rather than starting `index_source_with_auth` after shutdown was
/// requested. Exercised directly against `SourceRunner::run` (not the full
/// claim loop) so the assertion is deterministic instead of racing a real
/// shutdown against the worker's claim/spawn timing.
#[tokio::test]
async fn source_runner_honors_cancellation_before_running() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));

    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::new("https://example.com/docs");
    let created = store
        .create(source_job_request(&request))
        .await
        .expect("create source job row");

    let claimed = UnifiedClaimedJob {
        job_id: created.job_id,
        kind: UnifiedJobKind::Source,
        attempt: 1,
        request_json: Some(serde_json::json!({"source_request": request})),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
    };

    let shutdown = CancellationToken::new();
    shutdown.cancel();

    let result = runner.run(&claimed, &store, &shutdown).await;
    let error = result.expect_err("canceled-before-running must return Err, not proceed");
    assert!(
        error.message.contains("canceled before running"),
        "expected the pre-flight cancellation message, got: {}",
        error.message
    );
}

/// `build_registry` now wires `Source` alongside the existing kinds.
#[tokio::test]
async fn build_registry_registers_source() {
    let (_tmp, cfg) = test_cfg().await;
    let registry = build_registry(&cfg).expect("build registry");
    assert!(registry.contains(UnifiedJobKind::Source));
}
