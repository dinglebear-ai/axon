use axon_api::source::*;

use super::*;

#[tokio::test]
async fn fake_job_store_tracks_status_events_and_heartbeats() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();

    JobStore::update_status(
        &store,
        JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            error: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        JobStore::get(&store, job.job_id)
            .await
            .unwrap()
            .unwrap()
            .phase,
        PipelinePhase::Embedding
    );

    assert_eq!(
        JobStore::capabilities(&store).await.unwrap().0.owner_crate,
        "axon-jobs"
    );
    JobStore::reset(&store).await.unwrap();
    assert!(JobStore::get(&store, job.job_id).await.unwrap().is_none());
}

#[tokio::test]
async fn fake_watch_store_creates_updates_lists_and_records_history() {
    let store = FakeJobWatchStore::new();
    let watch = WatchStore::create(
        &store,
        WatchRequest {
            source: "file:///repo".to_string(),
            schedule: WatchSchedule {
                every_seconds: 60,
                cron: None,
                timezone: None,
            },
            embed: true,
            options: AdapterOptions::default(),
            scope: Some(SourceScope::Directory),
            collection: None,
            enabled: Some(true),
        },
    )
    .await
    .unwrap();

    let updated = WatchStore::update(
        &store,
        watch.watch_id.clone(),
        WatchUpdateRequest {
            schedule: None,
            enabled: Some(false),
            embed: None,
            scope: None,
            options: None,
        },
    )
    .await
    .unwrap();
    assert!(!updated.enabled);

    let listed = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: Some(false),
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(listed.items.len(), 1);
    WatchStore::reset(&store).await.unwrap();
    let empty = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert!(empty.items.is_empty());
}
