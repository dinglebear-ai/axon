use super::*;

/// Compile-level check: `JobServiceImpl` satisfies `JobService` and can be
/// held as a trait object. No live providers are touched — this only proves
/// the production impl (which wraps every `unified_ops` free function) type-
/// checks against the trait.
#[allow(dead_code)]
fn assert_job_service_impl_is_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn JobService> {
    Arc::new(JobServiceImpl::new(ctx))
}

fn fake_service() -> Arc<dyn JobService> {
    Arc::new(FakeJobService::new())
}

fn seed_fake(fake: &FakeJobService, status: LifecycleStatus) -> JobId {
    let job_id = JobId(uuid::Uuid::new_v4());
    fake.seed(fake_job_summary(job_id, status));
    job_id
}

#[tokio::test]
async fn fake_job_service_get_returns_seeded_job() {
    let fake = FakeJobService::new();
    let job_id = seed_fake(&fake, LifecycleStatus::Running);

    let got = fake.get(job_id).await.expect("job should exist");
    assert_eq!(got.job_id, job_id);
    assert_eq!(got.status, LifecycleStatus::Running);
}

#[tokio::test]
async fn fake_job_service_get_missing_errors() {
    let fake = fake_service();
    let err = fake
        .get(JobId(uuid::Uuid::new_v4()))
        .await
        .expect_err("get should fail for unknown job id");
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn fake_job_service_list_reflects_seeded_jobs() {
    let fake = FakeJobService::new();
    seed_fake(&fake, LifecycleStatus::Running);
    seed_fake(&fake, LifecycleStatus::Completed);

    let page = fake
        .list(JobListRequest {
            status: None,
            kind: None,
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list should succeed");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, Some(2));
}

#[tokio::test]
async fn fake_job_service_events_returns_empty_page() {
    let fake = FakeJobService::new();
    let job_id = seed_fake(&fake, LifecycleStatus::Running);

    let page = fake
        .events(JobEventListRequest {
            job_id,
            after_sequence: None,
            limit: None,
            cursor: None,
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
        })
        .await
        .expect("events should succeed");
    assert!(page.events.is_empty());
}

#[tokio::test]
async fn fake_job_service_cancel_updates_status() {
    let fake = FakeJobService::new();
    let job_id = seed_fake(&fake, LifecycleStatus::Running);

    let canceled = fake.cancel(job_id).await.expect("cancel should succeed");
    assert_eq!(canceled.status, LifecycleStatus::Canceled);
}

#[tokio::test]
async fn fake_job_service_retry_requeues_job() {
    let fake = FakeJobService::new();
    let job_id = seed_fake(&fake, LifecycleStatus::Failed);

    let descriptor = fake.retry(job_id).await.expect("retry should succeed");
    assert_eq!(descriptor.status, LifecycleStatus::Queued);
    let got = fake
        .get(job_id)
        .await
        .expect("job should exist after retry");
    assert_eq!(got.status, LifecycleStatus::Queued);
}

#[tokio::test]
async fn fake_job_service_recover_returns_zero_by_default() {
    let fake = fake_service();
    let result = fake
        .recover(JobRecoverRequest {
            kind: None,
            stale_before: None,
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .expect("recover should succeed");
    assert_eq!(result.recovered, 0);
}

#[tokio::test]
async fn fake_job_service_cleanup_dry_run_echoes_flag() {
    let fake = fake_service();
    let result = fake
        .cleanup(JobCleanupRequest {
            dry_run: true,
            kind: None,
            older_than: None,
            status: None,
            limit: None,
            older_than_seconds: None,
            confirm_all_terminal: false,
        })
        .await
        .expect("cleanup should succeed");
    assert!(result.dry_run);
}

#[tokio::test]
async fn fake_job_service_clear_requires_confirm() {
    let fake = fake_service();
    let request = JobClearRequest {
        status: None,
        confirm: false,
        kind: None,
        older_than: None,
    };
    assert!(fake.clear(request).await.is_err());
}

#[tokio::test]
async fn fake_job_service_clear_confirmed_empties_jobs() {
    let fake = FakeJobService::new();
    seed_fake(&fake, LifecycleStatus::Completed);

    let deleted = fake
        .clear(JobClearRequest {
            status: None,
            confirm: true,
            kind: None,
            older_than: None,
        })
        .await
        .expect("clear should succeed when confirmed");
    assert_eq!(deleted.deleted, 1);

    let page = fake
        .list(JobListRequest {
            status: None,
            kind: None,
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list should succeed");
    assert!(page.items.is_empty());
}
