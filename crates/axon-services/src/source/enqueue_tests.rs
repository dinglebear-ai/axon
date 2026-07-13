use axon_api::source::{AuthSnapshot, LifecycleStatus, SourceRequest};
use axon_jobs::boundary::FakeJobWatchStore;

use super::*;

#[tokio::test]
async fn detached_source_request_creates_a_queued_source_job() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("https://example.com/docs");

    let result = enqueue_source(request, &store, None)
        .await
        .expect("enqueue");

    assert_eq!(result.status, LifecycleStatus::Queued);
    let job = result.job.expect("job descriptor present");
    assert_eq!(job.kind, axon_api::source::JobKind::Source);
    assert_eq!(job.status, LifecycleStatus::Queued);
}

#[tokio::test]
async fn enqueued_job_request_json_carries_the_source_request() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("https://example.com/docs");

    let result = enqueue_source(request, &store, None)
        .await
        .expect("enqueue");
    let job = result.job.expect("job descriptor present");

    let request_json = axon_jobs::boundary::JobStore::request_json(&store, job.job_id)
        .await
        .expect("request json lookup")
        .expect("request json present");
    let source_request = request_json
        .get("source_request")
        .expect("source_request key present");
    assert_eq!(
        source_request.get("source").and_then(|v| v.as_str()),
        Some("https://example.com/docs")
    );
}

/// The `JobCreateRequest` builder (the piece actually plumbed to the job
/// store) must carry the caller-supplied `AuthSnapshot` verbatim — this is
/// what lets `SourceRunner` thread the real caller identity into
/// `index_source_with_auth` instead of a synthesized one.
#[test]
fn job_create_request_carries_the_caller_auth_snapshot() {
    let request = SourceRequest::new("https://example.com/docs");
    let auth_snapshot = AuthSnapshot::trusted_system("test-policy");

    let create_request = job_create_request(&request, auth_snapshot.clone());

    assert_eq!(
        create_request.auth_snapshot.caller_id,
        auth_snapshot.caller_id
    );
    assert_eq!(
        create_request.auth_snapshot.policy_version,
        auth_snapshot.policy_version
    );
}

#[tokio::test]
async fn matching_idempotency_key_returns_the_same_job_instead_of_a_duplicate() {
    let store = FakeJobWatchStore::new();
    let mut request = SourceRequest::new("https://example.com/docs");
    request.idempotency_key = Some("idem-key-1".to_string());

    let first = enqueue_source(request.clone(), &store, None)
        .await
        .expect("first enqueue");
    let second = enqueue_source(request, &store, None)
        .await
        .expect("second enqueue");

    assert_eq!(
        first.job.expect("first job").job_id,
        second.job.expect("second job").job_id
    );
}

#[tokio::test]
async fn empty_source_input_does_not_enqueue_a_job() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("   ");

    let result = enqueue_source(request, &store, None)
        .await
        .expect("enqueue");

    assert!(result.job.is_none());
    assert_eq!(result.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn enqueue_source_local_path_denied_without_local_scope() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::local_path("/tmp/axon-local-source", false);
    let mut auth = AuthSnapshot::default();
    auth.granted_scopes = vec![
        axon_api::source::AuthScope::Read,
        axon_api::source::AuthScope::Write,
    ];

    let result = enqueue_source(request, &store, Some(auth))
        .await
        .expect("enqueue should return failed source result");

    assert!(result.job.is_none());
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "auth.scope_required"),
        "missing local-scope warning: {:?}",
        result.warnings
    );
}
