use std::sync::Arc;

use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent, JobKind, JobPriority,
    JobStagePlan, LifecycleStatus, MetadataMap, PipelinePhase,
};
use axon_core::config::Config;
use axon_jobs::backend::JobKind as BackendJobKind;
use axon_jobs::boundary::JobStore;
use serde_json::json;

use crate::test_support::source_context_with_fake_web;

fn legacy_crawl_request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Crawl,
        job_intent: JobIntent::Index,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: Some("legacy-crawl-unreachable-test".to_string()),
        request: Some(json!({
            "urls": ["https://example.com"],
            "config_json": "{}"
        })),
        auth_snapshot: AuthSnapshot::trusted_system("legacy-crawl-unreachable-test"),
        config_snapshot_id: Some(ConfigSnapshotId::new("legacy_crawl_test")),
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Fetching,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        requirements: MetadataMap::default(),
        result_schema: None,
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::default(),
        deadline_at: None,
    }
}

fn source_job_request(idempotency_key: &str) -> JobCreateRequest {
    let mut request = legacy_crawl_request();
    request.job_kind = JobKind::Source;
    request.idempotency_key = Some(idempotency_key.to_string());
    request.request = Some(json!({
        "source_request": {
            "source": "https://example.com",
            "scope": "site",
            "embed": true
        }
    }));
    request
}

#[tokio::test]
async fn legacy_crawl_unreachable_registry_has_no_crawl_runner() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let registry =
        crate::runtime::job_runners::build_registry(&Arc::new(cfg)).expect("registry should build");

    assert!(registry.contains(JobKind::Source));
    assert!(!registry.contains(JobKind::Crawl));
}

#[tokio::test]
async fn legacy_crawl_unreachable_rows_are_dead_lettered_not_recovered() {
    let harness = source_context_with_fake_web().await.expect("harness");
    let store = harness.ctx().job_store().expect("job store");
    let created = store
        .create(legacy_crawl_request())
        .await
        .expect("legacy crawl row");

    let recovered = harness
        .ctx()
        .jobs
        .recover_jobs(BackendJobKind::Crawl, 0)
        .await
        .expect("recover legacy crawl rows");

    assert_eq!(recovered, 1);
    let row = store
        .get(created.job_id)
        .await
        .expect("get legacy row")
        .expect("legacy row should exist");
    assert_eq!(row.status, LifecycleStatus::Failed);
    assert_eq!(
        row.last_error.as_ref().map(|error| error.code.as_str()),
        Some("legacy.crawl.removed")
    );
}

#[tokio::test]
async fn legacy_crawl_unreachable_start_enqueues_source_not_crawl() {
    let harness = source_context_with_fake_web().await.expect("harness");
    let store = harness.ctx().job_store().expect("job store");
    let mut cfg = Config::test_default();
    cfg.start_url = "https://example.com".to_string();

    let outcome = crate::crawl::crawl_start_with_context(
        &cfg,
        &[cfg.start_url.clone()],
        harness.ctx(),
        None,
        None,
    )
    .await
    .expect("crawl start should enqueue source job");

    assert_eq!(outcome.result.jobs.len(), 1);
    assert_eq!(
        harness
            .jobs_by_kind(JobKind::Source)
            .await
            .expect("source jobs")
            .len(),
        1
    );
    assert!(
        harness
            .jobs_by_kind(JobKind::Crawl)
            .await
            .expect("crawl jobs")
            .is_empty()
    );
    assert!(
        store
            .list(axon_api::source::JobListRequest {
                status: None,
                kind: Some(JobKind::Crawl),
                source_id: None,
                watch_id: None,
                limit: Some(10),
                cursor: None,
            })
            .await
            .expect("list crawl jobs")
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn crawl_status_falls_back_to_unified_source_job() {
    let harness = source_context_with_fake_web().await.expect("harness");
    let store = harness.ctx().job_store().expect("job store");
    let created = store
        .create(source_job_request("crawl-status-source-fallback"))
        .await
        .expect("source job row");
    let job_id = created.job_id;

    let result = crate::crawl::crawl_status(harness.ctx(), job_id.0)
        .await
        .expect("crawl status fallback")
        .expect("unified source job status");

    assert_eq!(
        result
            .payload
            .get("status")
            .and_then(|value| value.as_str()),
        Some("pending")
    );
    assert_eq!(
        store
            .get(job_id)
            .await
            .expect("get source row")
            .expect("source row should exist")
            .job_id,
        job_id
    );
}

#[tokio::test]
async fn crawl_cancel_falls_back_to_unified_source_job() {
    let harness = source_context_with_fake_web().await.expect("harness");
    let store = harness.ctx().job_store().expect("job store");
    let created = store
        .create(source_job_request("crawl-cancel-source-fallback"))
        .await
        .expect("source job row");
    let job_id = created.job_id;

    let canceled = crate::crawl::crawl_cancel(harness.ctx(), job_id.0)
        .await
        .expect("crawl cancel fallback");

    assert!(canceled);
    let row = store
        .get(job_id)
        .await
        .expect("get canceled row")
        .expect("source row should exist");
    assert_eq!(row.status, LifecycleStatus::Canceled);
}
