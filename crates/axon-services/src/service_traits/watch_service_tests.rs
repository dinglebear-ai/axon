use super::*;

/// Compile-level check: `WatchServiceImpl` satisfies `WatchService` and can
/// be held as a trait object. No live providers are touched — this only
/// proves the production impl (including the `exec`/`history` wraps) type-
/// checks against the trait.
#[allow(dead_code)]
fn assert_watch_service_impl_is_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn WatchService> {
    Arc::new(WatchServiceImpl::new(ctx))
}

fn sample_request() -> WatchRequest {
    WatchRequest {
        source: "https://example.com/docs".to_string(),
        schedule: axon_api::source::WatchSchedule {
            every_seconds: 3600,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: axon_api::source::AdapterOptions::default(),
        scope: None,
        collection: None,
        enabled: Some(true),
    }
}

fn fake_service() -> Arc<dyn WatchService> {
    Arc::new(FakeWatchService::new())
}

#[tokio::test]
async fn fake_watch_service_create_then_get() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let got = fake
        .get(created.watch_id.clone())
        .await
        .expect("get should find created watch");
    assert_eq!(got.watch_id, created.watch_id);
    assert!(got.enabled);
}

#[tokio::test]
async fn fake_watch_service_update_changes_enabled_and_schedule() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let updated = fake
        .update(
            created.watch_id.clone(),
            WatchUpdateRequest {
                enabled: Some(false),
                schedule: Some(axon_api::source::WatchSchedule {
                    every_seconds: 7200,
                    cron: None,
                    timezone: None,
                }),
                options: None,
                embed: None,
                collection: None,
                scope: None,
            },
        )
        .await
        .expect("update should succeed");
    assert!(!updated.enabled);
    assert_eq!(updated.schedule.every_seconds, 7200);
}

#[tokio::test]
async fn fake_watch_service_list_reflects_created_watch() {
    let fake = fake_service();
    fake.create(sample_request())
        .await
        .expect("create should succeed");
    let page = fake
        .list(WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list should succeed");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, Some(1));
}

#[tokio::test]
async fn fake_watch_service_exec_returns_job_descriptor() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let job = fake
        .exec(
            created.watch_id,
            WatchExecRequest {
                reason: None,
                refresh: None,
                wait: None,
            },
        )
        .await
        .expect("exec should succeed");
    assert_eq!(job.status, LifecycleStatus::Queued);
    assert_eq!(job.kind, JobKind::Watch);
}

#[tokio::test]
async fn fake_watch_service_exec_missing_watch_errors() {
    let fake = fake_service();
    let err = fake
        .exec(
            WatchId::new("missing"),
            WatchExecRequest {
                reason: None,
                refresh: None,
                wait: None,
            },
        )
        .await
        .expect_err("exec should fail for unknown watch id");
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn fake_watch_service_pause_resume() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let paused = fake
        .pause(created.watch_id.clone())
        .await
        .expect("pause should succeed");
    assert!(!paused.enabled);
    let resumed = fake
        .resume(created.watch_id)
        .await
        .expect("resume should succeed");
    assert!(resumed.enabled);
}

#[tokio::test]
async fn fake_watch_service_delete_removes_watch() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let result = fake
        .delete(created.watch_id.clone())
        .await
        .expect("delete should succeed");
    assert!(result.deleted);
    assert!(fake.get(created.watch_id).await.is_err());
}

#[tokio::test]
async fn fake_watch_service_history_returns_empty_by_default() {
    let fake = fake_service();
    let created = fake
        .create(sample_request())
        .await
        .expect("create should succeed");
    let history = fake
        .history(WatchHistoryRequest {
            watch_id: created.watch_id.clone(),
            limit: None,
            cursor: None,
            status: None,
        })
        .await
        .expect("history should succeed");
    assert_eq!(history.watch_id, created.watch_id);
    assert!(history.jobs.is_empty());
}
