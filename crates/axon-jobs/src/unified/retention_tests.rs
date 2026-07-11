use axon_api::source::*;

use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use crate::unified::SqliteUnifiedJobStore;
use crate::unified::retention::RetentionCutoffs;

fn create_request(idempotency_key: &str) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: Some(idempotency_key.to_string()),
        stage_plan: Vec::new(),
        request: None,
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: None,
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// A terminal job row older than the terminal cutoff is deleted by the
/// retention sweep; a fresh terminal job (well inside the cutoff window) is
/// left alone.
#[tokio::test]
async fn retention_sweep_prunes_only_stale_terminal_jobs() {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    let store = SqliteUnifiedJobStore::new(pool.clone());

    let old = store
        .create(create_request("retention-old"))
        .await
        .expect("create old job");
    let fresh = store
        .create(create_request("retention-fresh"))
        .await
        .expect("create fresh job");

    // Force both jobs terminal, then backdate only `old` past every cutoff.
    for job_id in [old.job_id, fresh.job_id] {
        sqlx::query("UPDATE jobs SET status = 'completed' WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .execute(&pool)
            .await
            .expect("mark completed");
    }
    sqlx::query("UPDATE jobs SET updated_at = '2000-01-01T00:00:00Z' WHERE job_id = ?")
        .bind(old.job_id.0.to_string())
        .execute(&pool)
        .await
        .expect("backdate old job");

    let cutoffs = RetentionCutoffs {
        terminal: Timestamp::from(chrono::Utc::now() - chrono::Duration::days(30)),
        event: Timestamp::from(chrono::Utc::now() - chrono::Duration::days(14)),
        failed_event: Timestamp::from(chrono::Utc::now() - chrono::Duration::days(60)),
        provider_health: Timestamp::from(chrono::Utc::now() - chrono::Duration::days(7)),
        artifact: Timestamp::from(chrono::Utc::now() - chrono::Duration::days(30)),
    };

    let result = store
        .run_retention_sweep(&cutoffs)
        .await
        .expect("retention sweep");
    assert_eq!(result.jobs_pruned, 1);

    assert!(
        store
            .get_job(old.job_id)
            .await
            .expect("query old")
            .is_none()
    );
    assert!(
        store
            .get_job(fresh.job_id)
            .await
            .expect("query fresh")
            .is_some()
    );
}
