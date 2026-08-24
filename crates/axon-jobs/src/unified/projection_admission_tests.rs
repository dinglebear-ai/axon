use super::*;
use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use std::sync::Arc;

async fn store() -> SqliteUnifiedJobStore {
    SqliteUnifiedJobStore::new(open_sqlite_pool(":memory:").await.unwrap())
}

fn request() -> JobCreateRequest {
    JobCreateRequest {
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
        stage_plan: vec![JobStagePlan::required(PipelinePhase::Fetching)],
        request: Some(serde_json::json!({"source":"https://example.test"})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: Some("SourceResult".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

fn item(key: &str, fingerprint: &str) -> ProjectionAdmissionItem {
    ProjectionAdmissionItem {
        operation: ProjectionOperation::Crawl,
        storage_key: key.to_string(),
        fingerprint: RequestFingerprintV1(fingerprint.to_string()),
        request: request(),
    }
}

fn batch(principal: &str, items: Vec<ProjectionAdmissionItem>) -> ProjectionBatchAdmission {
    ProjectionBatchAdmission {
        batch_id: BatchId::new(Uuid::new_v4()),
        principal_id: principal.to_string(),
        items,
    }
}

#[tokio::test]
async fn projection_admission_rolls_back_every_job_on_collision() {
    let store = store().await;
    store
        .admit_projection_batch_atomic(batch("p1", vec![item("existing", "fp-a")]))
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    let result = store
        .admit_projection_batch_atomic(batch(
            "p1",
            vec![item("new", "fp-new"), item("existing", "fp-b")],
        ))
        .await;
    assert!(
        result
            .unwrap_err()
            .code
            .0
            .as_str()
            .contains("idempotency_collision")
    );
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn projection_admission_reuses_same_fingerprint_in_order() {
    let store = store().await;
    let admission = batch("principal-a", vec![item("same", "fp"), item("same", "fp")]);
    let result = store
        .admit_projection_batch_atomic(admission.clone())
        .await
        .unwrap();
    assert!(!result.items[0].reused);
    assert!(result.items[1].reused);
    assert_eq!(
        result.items[0].descriptor.job_id,
        result.items[1].descriptor.job_id
    );
    let lookup = store
        .projection_batch(ProjectionBatchLookup {
            batch_id: admission.batch_id,
            principal_id: "principal-a".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lookup.items.len(), 2);
}

#[tokio::test]
async fn projection_batch_lookup_is_principal_scoped() {
    let store = store().await;
    let admission = batch("principal-a", vec![item("key", "fp")]);
    store
        .admit_projection_batch_atomic(admission.clone())
        .await
        .unwrap();
    assert!(
        store
            .projection_batch(ProjectionBatchLookup {
                batch_id: admission.batch_id,
                principal_id: "principal-b".to_string(),
            })
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn reused_job_can_belong_to_multiple_batches() {
    let store = store().await;
    let first = batch("principal-a", vec![item("key", "fp")]);
    let second = batch("principal-a", vec![item("key", "fp")]);
    let first_result = store.admit_projection_batch_atomic(first).await.unwrap();
    let second_result = store
        .admit_projection_batch_atomic(second.clone())
        .await
        .unwrap();
    assert!(second_result.items[0].reused);
    assert_eq!(
        first_result.items[0].descriptor.job_id,
        second_result.items[0].descriptor.job_id
    );
    assert!(
        store
            .projection_batch(ProjectionBatchLookup {
                batch_id: second.batch_id,
                principal_id: "principal-a".to_string(),
            })
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn concurrent_same_fingerprint_admissions_reuse_one_job() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("projection-concurrent.db");
    let store = Arc::new(SqliteUnifiedJobStore::new(
        open_sqlite_pool(path.to_str().unwrap()).await.unwrap(),
    ));
    let first = batch("principal-a", vec![item("same", "fp")]);
    let second = batch("principal-a", vec![item("same", "fp")]);

    let (first_result, second_result) = tokio::join!(
        store.admit_projection_batch_atomic(first),
        store.admit_projection_batch_atomic(second),
    );
    let results = [first_result.unwrap(), second_result.unwrap()];

    assert_eq!(
        results[0].items[0].descriptor.job_id,
        results[1].items[0].descriptor.job_id
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result.items[0].reused)
            .count(),
        1
    );
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    let batch_item_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projection_batch_items")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    assert_eq!(job_count, 1);
    assert_eq!(batch_item_count, 2);
}

#[tokio::test]
async fn concurrent_idempotency_collision_leaves_only_winning_batch() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("projection-collision.db");
    let store = Arc::new(SqliteUnifiedJobStore::new(
        open_sqlite_pool(path.to_str().unwrap()).await.unwrap(),
    ));
    let first = batch("principal-a", vec![item("same", "fp-a")]);
    let second = batch("principal-a", vec![item("same", "fp-b")]);

    let (first_result, second_result) = tokio::join!(
        store.admit_projection_batch_atomic(first),
        store.admit_projection_batch_atomic(second),
    );
    let results = [first_result, second_result];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let error = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("one admission must collide");
    assert_eq!(error.code.0, "projection.idempotency_collision");
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    let batch_item_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projection_batch_items")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    assert_eq!(job_count, 1);
    assert_eq!(batch_item_count, 1);
}

#[tokio::test]
async fn file_backed_reopen_preserves_reuse_and_principal_scoped_lookup() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("projection-reopen.db");
    let first_store =
        SqliteUnifiedJobStore::new(open_sqlite_pool(path.to_str().unwrap()).await.unwrap());
    let original = batch("principal-a", vec![item("stable", "fp")]);
    let original_batch_id = original.batch_id;
    let original_result = first_store
        .admit_projection_batch_atomic(original)
        .await
        .unwrap();
    first_store.pool_for_tests().close().await;

    let reopened =
        SqliteUnifiedJobStore::new(open_sqlite_pool(path.to_str().unwrap()).await.unwrap());
    let reused = reopened
        .admit_projection_batch_atomic(batch("principal-a", vec![item("stable", "fp")]))
        .await
        .unwrap();
    assert!(reused.items[0].reused);
    assert_eq!(
        reused.items[0].descriptor.job_id,
        original_result.items[0].descriptor.job_id
    );
    assert!(
        reopened
            .projection_batch(ProjectionBatchLookup {
                batch_id: original_batch_id,
                principal_id: "principal-a".to_string(),
            })
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        reopened
            .projection_batch(ProjectionBatchLookup {
                batch_id: original_batch_id,
                principal_id: "principal-b".to_string(),
            })
            .await
            .unwrap()
            .is_none()
    );
}
