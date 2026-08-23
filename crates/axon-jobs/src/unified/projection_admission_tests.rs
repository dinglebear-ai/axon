use super::*;
use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;

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
