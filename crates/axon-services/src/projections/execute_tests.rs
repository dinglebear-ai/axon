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
