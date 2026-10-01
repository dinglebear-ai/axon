use super::*;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_vectors::point::{
    RedactionSkipCounts, RedactionSkipDetector, RedactionSkipField, RedactionSkipReason,
};
use std::sync::Arc;

#[tokio::test]
async fn rejected_chunk_reason_reaches_job_warning_without_matched_secret() {
    let mut document = axon_vectors::testing::test_prepared_document();
    let sensitive = "API_KEY=abcdef0123456789abcdef0123"; // gitleaks:allow -- synthetic detector fixture
    document.chunks[1].content = sensitive.into();
    let mut embeddings =
        axon_vectors::testing::test_embedding_result_for(&document, "text-embedding-test", 3);
    let built = point_batch(
        axon_vectors::testing::test_collection_spec(3),
        std::slice::from_ref(&document),
        &mut embeddings,
    )
    .unwrap();
    assert_eq!(built.skipped_redaction, 1);
    let result = vectorize_result(
        vec![document],
        Vec::new(),
        &built.points_by_document,
        vector_write(1),
        built.skipped_redaction,
        &built.redaction_skips_by_source_item,
    );
    let warning = result.warnings.first().unwrap();
    assert!(warning.message.contains("secret_assignment"));
    assert!(warning.message.contains("body"));
    assert!(warning.message.contains("skipped 1 chunk"));
    assert!(!serde_json::to_string(warning).unwrap().contains(sensitive));
    assert!(!warning.message.contains("abcdef0123456789abcdef0123")); // gitleaks:allow -- synthetic detector fixture
    let jobs = Arc::new(FakeJobWatchStore::new());
    let job = jobs.create(job_request()).await.unwrap();
    SourceEventEmitter::new(Some(jobs.clone()), Some(job.job_id))
        .warning(
            PipelinePhase::Vectorizing,
            warning.clone(),
            Some(SourceGenerationId::new("gen_1")),
        )
        .await;
    let events = jobs
        .events(JobEventListRequest {
            job_id: job.job_id,
            after_sequence: None,
            limit: Some(10),
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await
        .unwrap()
        .events;
    assert_eq!(events.len(), 1);
    let persisted = serde_json::to_string(&events).unwrap();
    assert!(persisted.contains("secret_assignment"));
    assert!(persisted.contains("body"));
    assert!(!persisted.contains("abcdef0123456789abcdef0123")); // gitleaks:allow -- synthetic detector fixture
}

#[test]
fn typed_reason_counts_keep_metadata_separate_and_preserve_unattributed_count() {
    let reasons: RedactionSkipCounts = [
        (
            RedactionSkipReason {
                field: RedactionSkipField::Body,
                detector: RedactionSkipDetector::SecretAssignment,
            },
            2,
        ),
        (
            RedactionSkipReason {
                field: RedactionSkipField::Metadata,
                detector: RedactionSkipDetector::ForbiddenFieldName,
            },
            1,
        ),
    ]
    .into_iter()
    .collect();
    let by_item = [(SourceItemKey::new("safe-item"), reasons)]
        .into_iter()
        .collect();
    let warnings = redaction_warnings::warnings(4, &by_item);
    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].message.contains("skipped 3 chunk"));
    assert!(warnings[0].message.contains("metadata"));
    assert!(warnings[0].message.contains("forbidden_field_name"));
    assert!(warnings[0].message.contains("secret_assignment"));
    assert!(warnings[1].message.contains("skipped 1 chunk"));
    assert!(warnings[1].source_item_key.is_none());
}

fn job_request() -> JobCreateRequest {
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
        idempotency_key: None,
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
