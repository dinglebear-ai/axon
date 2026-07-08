use super::*;
use crate::context::ServiceContext;

async fn test_ctx() -> ServiceContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..axon_core::config::Config::test_default()
    };
    // Leak the tempdir so the DB file survives for the life of the test — an
    // in-process short-lived ServiceContext test, not a long-running one.
    std::mem::forget(dir);
    ServiceContext::new(std::sync::Arc::new(cfg))
        .await
        .expect("service context")
}

/// `doctor` is a `provider_probe` job-backed operation (see
/// `docs/pipeline-unification/runtime/job-contract.md`). Running it must
/// create a unified job row that ends up `Completed` — mirrors
/// `memory::tests::dispatch_covers_full_lifecycle_surface`'s job-tracking
/// assertion for `MemoryCompaction`.
#[tokio::test]
async fn doctor_run_creates_completed_provider_probe_job() {
    let ctx = test_ctx().await;

    // Doctor's own health checks (Qdrant/TEI/LLM reachability) may fail in
    // this offline test environment — that's fine, `doctor` itself never
    // errors on unreachable dependencies, it just reports them as unhealthy
    // in the payload. What this test asserts is the job-tracking side effect.
    let result = doctor(&ctx).await.expect("doctor completes");
    assert!(result.payload.is_object());

    let jobs = ctx
        .job_store()
        .expect("unified job store")
        .list(axon_api::source::JobListRequest {
            status: None,
            kind: Some(axon_api::source::JobKind::ProviderProbe),
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list jobs");

    assert_eq!(jobs.items.len(), 1);
    assert_eq!(
        jobs.items[0].status,
        axon_api::source::LifecycleStatus::Completed
    );
    assert_eq!(jobs.items[0].kind, axon_api::source::JobKind::ProviderProbe);
}

/// Running `doctor` twice creates two independent provider_probe jobs — each
/// health check is its own tracked operation, not shared/deduped state.
#[tokio::test]
async fn doctor_run_twice_creates_two_jobs() {
    let ctx = test_ctx().await;

    doctor(&ctx).await.expect("first doctor run");
    doctor(&ctx).await.expect("second doctor run");

    let jobs = ctx
        .job_store()
        .expect("unified job store")
        .list(axon_api::source::JobListRequest {
            status: None,
            kind: Some(axon_api::source::JobKind::ProviderProbe),
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list jobs");

    assert_eq!(jobs.items.len(), 2);
    assert!(
        jobs.items
            .iter()
            .all(|job| job.status == axon_api::source::LifecycleStatus::Completed)
    );
}
