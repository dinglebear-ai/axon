use super::*;
use axon_api::source::{JobId, SourceGenerationId, SourceId};

fn record() -> GraphStageRecord {
    GraphStageRecord {
        stage_id: uuid::Uuid::new_v4().to_string(),
        source_id: SourceId::new("cleanup-source"),
        generation_id: SourceGenerationId::new("cleanup-generation"),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        attempt: 2,
        state: "building".into(),
    }
}

#[test]
fn only_settled_attempts_or_published_stages_are_eligible() {
    let mut stage = record();
    assert!(!eligible(&stage, None));
    for status in [
        LifecycleStatus::Running,
        LifecycleStatus::Queued,
        LifecycleStatus::Canceling,
    ] {
        assert!(!eligible(&stage, Some((status, 2))));
    }
    assert!(eligible(&stage, Some((LifecycleStatus::Failed, 2))));
    assert!(eligible(&stage, Some((LifecycleStatus::Running, 3))));
    assert!(!eligible(&stage, Some((LifecycleStatus::Completed, 1))));
    stage.state = "activated".into();
    assert!(eligible(&stage, None));
}

#[tokio::test]
async fn disposal_preserves_active_owner_then_retries_and_keeps_receipt() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let stage = record();
    let handle = GraphStage::begin(
        pool.clone(),
        stage.source_id.clone(),
        stage.generation_id.clone(),
        stage.job_id,
        stage.attempt,
    )
    .await
    .unwrap();
    let jobs = axon_jobs::boundary::FakeJobWatchStore::new();
    sweep(&pool, &jobs).await;
    assert_eq!(
        GraphStage::list(&pool).await.unwrap().len(),
        1,
        "unknown active owner must be preserved"
    );
    // An activated stage is disposable, but its still-live handle holds the
    // owner lock. Recovery must retry after that writer settles.
    sqlx::query("UPDATE graph_stages SET state='activated' WHERE stage_id=?")
        .bind(handle.id())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO graph_stage_receipts(stage_id,source_id,generation_id,summary_json) VALUES(?,?,?,?)").bind(handle.id()).bind(&stage.source_id.0).bind(&stage.generation_id.0).bind(serde_json::to_string(&handle.summary().await.unwrap()).unwrap()).execute(&pool).await.unwrap();
    sweep(&pool, &jobs).await;
    assert_eq!(
        GraphStage::list(&pool).await.unwrap().len(),
        1,
        "locked stage must retain retry record"
    );
    handle.mark_disposable().await.unwrap();
    sweep(&pool, &jobs).await;
    assert!(GraphStage::list(&pool).await.unwrap().is_empty());
    assert!(
        GraphStage::activation_summary(&pool, &stage.source_id, &stage.generation_id)
            .await
            .unwrap()
            .is_some(),
        "publication receipt survives physical disposal"
    );
    pool.close().await;
}

#[tokio::test]
async fn failed_job_stage_is_recovered_after_handle_settles() {
    use axon_api::source::*;
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let jobs = axon_jobs::boundary::FakeJobWatchStore::new();
    let job = jobs
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Run,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 0,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![],
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("stage-cleanup-test"),
            config_snapshot_id: None,
            requirements: Default::default(),
            result_schema: None,
            warnings: vec![],
            error: None,
            metadata: Default::default(),
            deadline_at: None,
        })
        .await
        .unwrap();
    let handle = GraphStage::begin(
        pool.clone(),
        SourceId::new("failed-source"),
        SourceGenerationId::new("failed-generation"),
        job.job_id,
        0,
    )
    .await
    .unwrap();
    sweep(&pool, &jobs).await;
    assert_eq!(GraphStage::list(&pool).await.unwrap().len(), 1);
    for status in [LifecycleStatus::Running, LifecycleStatus::Failed] {
        jobs.update_status(JobStatusUpdate {
            job_id: job.job_id,
            source_id: None,
            status,
            phase: PipelinePhase::Preparing,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .unwrap();
    }
    drop(handle);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            sweep(&pool, &jobs).await;
            if GraphStage::list(&pool).await.unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("closed private pool eventually releases disposal ownership");
    pool.close().await;
}

#[tokio::test]
async fn filesystem_disposal_failure_retains_record_for_next_sweep() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let stage = record();
    let handle = GraphStage::begin(
        pool.clone(),
        stage.source_id,
        stage.generation_id,
        stage.job_id,
        stage.attempt,
    )
    .await
    .unwrap();
    let path: String = sqlx::query_scalar("SELECT path FROM graph_stages WHERE stage_id=?")
        .bind(handle.id())
        .fetch_one(&pool)
        .await
        .unwrap();
    handle.mark_disposable().await.unwrap();
    let obstruction = format!("{path}-shm");
    std::fs::create_dir(&obstruction).unwrap();
    let jobs = axon_jobs::boundary::FakeJobWatchStore::new();
    sweep(&pool, &jobs).await;
    assert_eq!(GraphStage::list(&pool).await.unwrap().len(), 1);
    std::fs::remove_dir(obstruction).unwrap();
    sweep(&pool, &jobs).await;
    assert!(GraphStage::list(&pool).await.unwrap().is_empty());
    pool.close().await;
}
