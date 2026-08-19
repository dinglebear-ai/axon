use axon_api::source::{AuthScope, AuthSnapshot, LifecycleStatus, SourceRequest};
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
async fn enqueue_source_local_path_allowed_for_trusted_cli() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::local_path("/tmp/axon-local-source", false);

    let result = enqueue_source(request, &store, Some(AuthSnapshot::trusted_cli("test")))
        .await
        .expect("enqueue should succeed");

    assert!(
        result.job.is_some(),
        "trusted CLI context must be allowed to detach local sources: {:?}",
        result.warnings
    );
    assert_eq!(result.status, LifecycleStatus::Queued);
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

#[tokio::test]
async fn detached_server_local_source_requires_a_configured_allowed_root() {
    let store = FakeJobWatchStore::new();
    let root = crate::test_support::visible_tempdir().expect("local source");
    let request = SourceRequest::local_path(root.path().to_string_lossy(), true);
    let mut auth = AuthSnapshot::default();
    auth.granted_scopes = vec![
        axon_api::source::AuthScope::Read,
        axon_api::source::AuthScope::Write,
        axon_api::source::AuthScope::Local,
    ];

    let denied =
        enqueue_source_with_allowed_roots(request.clone(), &store, Some(auth.clone()), Some(&[]))
            .await
            .expect("deny result");
    assert!(denied.job.is_none());
    assert!(
        denied
            .warnings
            .iter()
            .any(|warning| warning.code == "security.local_root_denied")
    );

    let allowed = enqueue_source_with_allowed_roots(
        request,
        &store,
        Some(auth),
        Some(&[root.path().to_path_buf()]),
    )
    .await
    .expect("allowed enqueue");
    assert!(allowed.job.is_some());
}

#[tokio::test]
async fn enqueue_source_tool_denied_without_execute_scope() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("cli:rg --help").without_embedding();
    let mut auth = AuthSnapshot::default();
    auth.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

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
        "missing execute-scope warning: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn enqueue_source_tool_with_execute_scope_is_routable() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("mcp:labby/search").without_embedding();
    let mut auth = AuthSnapshot::default();
    auth.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Execute];

    let result = enqueue_source_with_access_policy(request, &store, Some(auth), None, true)
        .await
        .expect("enqueue");

    assert_eq!(result.status, LifecycleStatus::Queued);
    assert_eq!(result.source_kind, axon_api::source::SourceKind::McpTool);
    assert_eq!(result.scope, axon_api::source::SourceScope::Tool);
    assert!(
        result
            .warnings
            .iter()
            .all(|warning| warning.code != "source.route.unsupported_dispatch"),
        "tool source should route to a live dispatch family: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn enqueue_source_tool_requires_operator_and_caller_authority() {
    let store = FakeJobWatchStore::new();
    let request = SourceRequest::new("mcp:labby/search").without_embedding();
    let mut execute_auth = AuthSnapshot::default();
    execute_auth.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Execute];

    let operator_denied =
        enqueue_source_with_access_policy(request.clone(), &store, Some(execute_auth), None, false)
            .await
            .expect("operator-disabled result");
    assert!(operator_denied.job.is_none());
    assert_eq!(operator_denied.status, LifecycleStatus::Failed);

    let caller_denied = enqueue_source_with_access_policy(
        request,
        &store,
        Some(AuthSnapshot::panel("panel-policy")),
        None,
        true,
    )
    .await
    .expect("caller-disabled result");
    assert!(caller_denied.job.is_none());
    assert_eq!(caller_denied.status, LifecycleStatus::Failed);
}

#[test]
fn source_job_create_request_persists_the_canonical_stage_spine() {
    let request = SourceRequest::new("https://example.com/docs");
    let create = job_create_request(&request, AuthSnapshot::trusted_system("stage-test"));

    assert!(!create.stage_plan.is_empty());
    assert_eq!(
        create.stage_plan.first().map(|stage| stage.phase),
        Some(axon_api::source::PipelinePhase::Leasing)
    );
    assert!(
        create
            .stage_plan
            .iter()
            .any(|stage| stage.phase == axon_api::source::PipelinePhase::Embedding)
    );
    assert!(
        create
            .stage_plan
            .iter()
            .any(|stage| stage.phase == axon_api::source::PipelinePhase::Publishing)
    );
    let keys = create
        .stage_plan
        .iter()
        .map(|stage| stage.effective_stage_key())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(keys.len(), create.stage_plan.len());
}
