use super::*;

#[test]
fn projection_execute_fingerprint_is_stable_and_semantic() {
    let value = serde_json::json!({"operation":"crawl","source":"https://example.test"});
    assert_eq!(digest_json(&value).unwrap(), digest_json(&value).unwrap());
    assert_ne!(
        digest_json(&value).unwrap(),
        digest_json(&serde_json::json!({"operation":"scrape","source":"https://example.test"}))
            .unwrap()
    );
}

#[test]
fn projection_execute_principal_is_opaque() {
    let mut auth = AuthSnapshot::default();
    auth.caller_id = Some("user@example.test".to_string());
    let digest = principal_digest(Some(&auth));
    assert_eq!(digest.len(), 64);
    assert!(!digest.contains("user"));
}

#[test]
fn projection_execute_principal_isolated_by_auth_realm() {
    let mut first = AuthSnapshot::default();
    first.caller_id = Some("shared-subject".to_string());
    first.policy_version = "issuer-a".to_string();
    let mut second = first.clone();
    second.policy_version = "issuer-b".to_string();
    assert_ne!(
        principal_digest(Some(&first)),
        principal_digest(Some(&second))
    );
}

#[test]
fn projection_fingerprint_uses_canonical_target_and_excludes_caller_key() {
    let mut request = SourceRequest::new("https://EXAMPLE.test:443/docs/../docs");
    request.idempotency_key = Some("caller-secret".to_string());
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let prepared = PreparedSourceItem {
        index: 0,
        request,
        kind: routed.kind,
        route: routed.route,
        required_scope: AuthScope::Write,
    };
    let first = admission_item(ProjectionOperation::Ingest, &prepared, "principal", None).unwrap();
    let mut equivalent = prepared.clone();
    equivalent.request.source = equivalent.route.source.canonical_uri.clone();
    equivalent.request.idempotency_key = Some("caller-secret".to_string());
    let second =
        admission_item(ProjectionOperation::Ingest, &equivalent, "principal", None).unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(first.fingerprint.0.len(), 64);
}

#[test]
fn queued_projection_item_never_echoes_sensitive_input() {
    let descriptor = JobDescriptor {
        kind: JobKind::Source,
        id: JobId::new(uuid::Uuid::nil()),
        job_id: JobId::new(uuid::Uuid::nil()),
        status_url: "/jobs/redacted".to_string(),
        events_url: "/jobs/redacted/events".to_string(),
        stream_url: "/jobs/redacted/stream".to_string(),
        poll_after_ms: 250,
        cancel_url: None,
        retry_url: None,
        status: LifecycleStatus::Queued,
        poll: None,
        created_at: None,
        updated_at: None,
    };
    let item = redacted_source_item(0, BatchOutcome::Queued(descriptor));
    let json = serde_json::to_string(&item).unwrap();
    assert!(item.input.is_none());
    assert!(!json.contains("sensitive"));
    assert!(!json.contains("input"));
}

#[test]
fn foreground_wait_policy_preserves_detached_queueing() {
    let mut execution = ExecutionPolicy::default();
    execution.mode = ExecutionMode::Foreground;
    assert!(should_wait(&execution));
    execution.detached = true;
    assert!(!should_wait(&execution));
    execution.detached = false;
    execution.mode = ExecutionMode::Wait;
    assert!(should_wait(&execution));
    execution.mode = ExecutionMode::Background;
    assert!(!should_wait(&execution));
}

#[test]
fn response_size_gate_rejects_oversized_completed_payload() {
    let value = serde_json::json!({"status":"completed","data":"sensitive-large-value"});
    let error = validate_response_size(&value, 8).unwrap_err();
    assert_eq!(error.code.0, "projection.response_too_large");
}

#[tokio::test]
async fn projection_admission_is_claimed_by_the_canonical_source_worker() {
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = axon_core::config::Config::test_default();
    cfg.sqlite_path = temp.path().join("projection-worker.db");
    cfg.qdrant_url.clear();
    cfg.tei_url.clear();
    let cfg = std::sync::Arc::new(cfg);
    let ctx = crate::context::ServiceContext::new_with_workers(std::sync::Arc::clone(&cfg))
        .await
        .expect("service context with canonical workers");
    let mut request = SourceRequest::new("https://example.com/projection-worker");
    request.execution.mode = ExecutionMode::Background;
    request.execution.detached = true;
    let prepared = crate::projections::preflight_source_batch(
        ProjectionOperation::Ingest,
        vec![request],
        None,
        &cfg.projection_batch,
        &crate::projections::SourceAccessPolicy::default(),
    )
    .expect("projection preflight");
    let batch = enqueue_source_projection_batch(&ctx, ProjectionOperation::Ingest, prepared, None)
        .await
        .expect("atomic projection admission");
    let descriptor = match &batch.items[0].outcome {
        BatchOutcome::Queued(descriptor) => descriptor.clone(),
        other => panic!("expected queued projection, got {other:?}"),
    };
    let store = ctx.job_store().expect("unified job store");
    let summary = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let summary = store
                .get(descriptor.job_id)
                .await
                .expect("read admitted job")
                .expect("admitted job exists");
            if matches!(
                summary.status,
                LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
                    | LifecycleStatus::Failed
                    | LifecycleStatus::Canceled
            ) {
                break summary;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("canonical source worker reaches terminal state");
    assert_eq!(summary.job_id, descriptor.job_id);
    assert_eq!(summary.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn foreground_projection_waits_for_the_admitted_job_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = axon_core::config::Config::test_default();
    cfg.sqlite_path = temp.path().join("projection-foreground.db");
    cfg.qdrant_url.clear();
    cfg.tei_url.clear();
    cfg.projection_batch.max_elapsed_secs = 10;
    let cfg = std::sync::Arc::new(cfg);
    let ctx = crate::context::ServiceContext::new_with_workers(std::sync::Arc::clone(&cfg))
        .await
        .expect("service context with canonical workers");
    let mut request = SourceRequest::new("https://example.com/projection-foreground");
    request.execution.mode = ExecutionMode::Foreground;
    request.execution.detached = false;
    let prepared = crate::projections::preflight_source_batch(
        ProjectionOperation::Ingest,
        vec![request],
        None,
        &cfg.projection_batch,
        &crate::projections::SourceAccessPolicy::default(),
    )
    .unwrap();
    let batch = execute_source_projection_batch(&ctx, ProjectionOperation::Ingest, prepared, None)
        .await
        .expect("foreground batch response");
    assert_eq!(batch.status, BatchStatus::CompletedDegraded);
    assert_eq!(batch.summary.failed, 1);
    assert!(matches!(batch.items[0].outcome, BatchOutcome::Failed(_)));
}

#[test]
fn mixed_code_search_outcomes_preserve_order_and_summary() {
    let items = vec![
        BatchItem {
            index: 0,
            input: Some("first".to_string()),
            outcome: BatchOutcome::Completed(QueryResult { results: vec![] }),
        },
        BatchItem {
            index: 1,
            input: Some("second".to_string()),
            outcome: BatchOutcome::Failed(ApiError::new(
                "projection.code_search_failed",
                ErrorStage::Retrieving,
                "failed",
            )),
        },
        BatchItem {
            index: 2,
            input: Some("third".to_string()),
            outcome: BatchOutcome::Completed(QueryResult { results: vec![] }),
        },
    ];
    let result = finish_code_search_batch(BatchId::new(uuid::Uuid::nil()), items, 64 * 1024)
        .expect("mixed result");
    assert_eq!(result.status, BatchStatus::CompletedDegraded);
    assert_eq!(result.summary.completed, 2);
    assert_eq!(result.summary.failed, 1);
    assert_eq!(
        result
            .items
            .iter()
            .map(|item| item.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}
