use super::*;

use axon_api::source::{AuthScope, JobKind, JobListRequest, LifecycleStatus, TransportKind};
use axon_jobs::boundary::FakeJobWatchStore;
use uuid::Uuid;

fn store() -> Arc<dyn JobStore> {
    Arc::new(FakeJobWatchStore::new())
}

/// A restricted, non-admin caller snapshot — read/write only, no admin/local/
/// execute scope. Mirrors a real end-user request (as opposed to a system-
/// triggered run).
fn restricted_caller_snapshot() -> AuthSnapshot {
    AuthSnapshot {
        caller_id: Some("user_1".to_string()),
        transport: TransportKind::Mcp,
        granted_scopes: vec![AuthScope::Read, AuthScope::Write],
        ..AuthSnapshot::default()
    }
}

fn nonempty_graph_summary(degraded: bool) -> GraphWriteSummary {
    GraphWriteSummary {
        nodes_upserted: 3,
        edges_upserted: 2,
        evidence_records: 2,
        degraded,
    }
}

fn empty_graph_summary() -> GraphWriteSummary {
    GraphWriteSummary {
        nodes_upserted: 0,
        edges_upserted: 0,
        evidence_records: 0,
        degraded: true,
    }
}

fn nonempty_drain_summary(failed: u64) -> DebtDrainSummary {
    DebtDrainSummary {
        resolved: 2,
        failed,
        points_deleted: 5,
    }
}

fn empty_drain_summary() -> DebtDrainSummary {
    DebtDrainSummary::default()
}

async fn only_job(store: &Arc<dyn JobStore>, kind: JobKind) -> axon_api::source::JobSummary {
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(kind),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert_eq!(page.items.len(), 1, "expected exactly one {kind:?} job");
    page.items.into_iter().next().unwrap()
}

#[tokio::test]
async fn track_graph_mutation_creates_completed_child_job_on_success() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_graph_mutation(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_graph_summary(false),
    )
    .await;

    let job = only_job(&store, JobKind::Graph).await;
    assert_eq!(job.status, LifecycleStatus::Completed);
    assert_eq!(job.parent_job_id, Some(parent_job_id));
    assert_eq!(job.root_job_id, Some(parent_job_id));
}

#[tokio::test]
async fn track_graph_mutation_marks_degraded_write_as_failed_child_job() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_graph_mutation(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_graph_summary(true),
    )
    .await;

    let job = only_job(&store, JobKind::Graph).await;
    assert_eq!(job.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn track_graph_mutation_skips_job_creation_for_zero_op_write() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_graph_mutation(
        Some(store.clone()),
        parent_job_id,
        None,
        &empty_graph_summary(),
    )
    .await;

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Graph),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert!(
        page.items.is_empty(),
        "zero-op graph write must not create a job row"
    );
}

#[tokio::test]
async fn track_graph_mutation_skips_when_no_job_store() {
    // Must not panic without a job store — this is the degraded/no-data-plane
    // path's shape (no assertions possible beyond "did not panic").
    track_graph_mutation(
        None,
        JobId::new(Uuid::new_v4()),
        None,
        &nonempty_graph_summary(false),
    )
    .await;
}

#[tokio::test]
async fn track_graph_mutation_skips_nil_parent_job_id() {
    let store = store();
    track_graph_mutation(
        Some(store.clone()),
        JobId::new(Uuid::nil()),
        None,
        &nonempty_graph_summary(false),
    )
    .await;

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Graph),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert!(
        page.items.is_empty(),
        "nil parent job id must not create a child job"
    );
}

#[tokio::test]
async fn track_prune_creates_completed_child_job_when_all_debt_resolved() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_prune(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_drain_summary(0),
    )
    .await;

    let job = only_job(&store, JobKind::Prune).await;
    assert_eq!(job.status, LifecycleStatus::Completed);
    assert_eq!(job.parent_job_id, Some(parent_job_id));
    assert_eq!(job.root_job_id, Some(parent_job_id));
}

#[tokio::test]
async fn track_prune_marks_partial_failure_as_failed_child_job() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_prune(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_drain_summary(1),
    )
    .await;

    let job = only_job(&store, JobKind::Prune).await;
    assert_eq!(job.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn track_prune_skips_job_creation_when_no_debt_touched() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());

    track_prune(
        Some(store.clone()),
        parent_job_id,
        None,
        &empty_drain_summary(),
    )
    .await;

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Prune),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert!(
        page.items.is_empty(),
        "empty debt drain must not create a job row"
    );
}

#[tokio::test]
async fn track_prune_skips_nil_parent_job_id() {
    let store = store();
    track_prune(
        Some(store.clone()),
        JobId::new(Uuid::nil()),
        None,
        &nonempty_drain_summary(0),
    )
    .await;

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Prune),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert!(
        page.items.is_empty(),
        "nil parent job id must not create a child job"
    );
}

// -- Security: child jobs must never gain more scope than the real caller --
//
// Before this fix, `child_job_request` hardcoded
// `AuthSnapshot::trusted_system("runtime")` for every child graph/prune job,
// granting Read+Write+Admin regardless of what the actual caller held. These
// tests prove a restricted (non-admin) caller's auth snapshot is inherited
// by the child job request — not silently escalated — and that the no-caller
// fallback also carries no elevated scope.

#[test]
fn child_job_request_inherits_restricted_parent_scopes_for_graph() {
    let parent_job_id = JobId::new(Uuid::new_v4());
    let parent = restricted_caller_snapshot();

    let request = child_job_request(
        parent_job_id,
        Some(&parent),
        JobKind::Graph,
        JobIntent::Run,
        PipelinePhase::Graphing,
        "graph_result",
    );

    assert_eq!(request.auth_snapshot.granted_scopes, parent.granted_scopes);
    assert!(
        !request
            .auth_snapshot
            .granted_scopes
            .contains(&AuthScope::Admin),
        "restricted caller's child graph job must not gain Admin scope"
    );
    assert_eq!(request.auth_snapshot.caller_id, parent.caller_id);
}

#[test]
fn child_job_request_inherits_restricted_parent_scopes_for_prune() {
    let parent_job_id = JobId::new(Uuid::new_v4());
    let parent = restricted_caller_snapshot();

    let request = child_job_request(
        parent_job_id,
        Some(&parent),
        JobKind::Prune,
        JobIntent::Cleanup,
        PipelinePhase::Cleaning,
        "prune_result",
    );

    assert_eq!(request.auth_snapshot.granted_scopes, parent.granted_scopes);
    assert!(
        !request
            .auth_snapshot
            .granted_scopes
            .contains(&AuthScope::Admin),
        "restricted caller's child prune job must not gain Admin scope"
    );
    assert_eq!(request.auth_snapshot.caller_id, parent.caller_id);
}

#[test]
fn child_job_request_without_parent_snapshot_gets_no_elevated_scope() {
    let parent_job_id = JobId::new(Uuid::new_v4());

    let request = child_job_request(
        parent_job_id,
        None,
        JobKind::Graph,
        JobIntent::Run,
        PipelinePhase::Graphing,
        "graph_result",
    );

    assert!(
        request.auth_snapshot.granted_scopes.is_empty(),
        "missing parent snapshot must fall back to no scopes, not trusted_system's \
         Read+Write+Admin"
    );
    assert!(
        !request
            .auth_snapshot
            .granted_scopes
            .contains(&AuthScope::Admin)
    );
}

/// End-to-end: `track_graph_mutation` accepts a restricted caller's auth
/// snapshot and completes normally. The fake store does not persist
/// `auth_snapshot` on `JobSummary`, so the exact snapshot contents are
/// asserted via the dedicated `child_job_request` tests above (the single
/// place both `track_graph_mutation`/`track_prune` construct it); this test
/// proves the plumbing end-to-end with a restricted (non-admin) parent.
#[tokio::test]
async fn track_graph_mutation_with_restricted_caller_does_not_create_admin_child_job() {
    let store = store();
    let parent_job_id = JobId::new(Uuid::new_v4());
    let parent = restricted_caller_snapshot();

    track_graph_mutation(
        Some(store.clone()),
        parent_job_id,
        Some(&parent),
        &nonempty_graph_summary(false),
    )
    .await;

    let job = only_job(&store, JobKind::Graph).await;
    assert_eq!(job.status, LifecycleStatus::Completed);
}

async fn completed_source_job(store: &Arc<dyn JobStore>) -> JobId {
    let descriptor = store
        .create(axon_api::source::JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: axon_api::source::JobIntent::Run,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: axon_api::source::JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![axon_api::source::JobStagePlan {
                phase: PipelinePhase::Fetching,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: None,
            }],
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: None,
            requirements: axon_api::source::MetadataMap::new(),
            result_schema: None,
            warnings: Vec::new(),
            error: None,
            metadata: axon_api::source::MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .expect("create parent source job");
    let job_id = descriptor.job_id;
    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Fetching,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("mark parent running");
    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("mark parent completed");
    job_id
}

#[tokio::test]
async fn child_jobs_attach_to_an_already_terminal_parent_source_job() {
    let store = store();
    let parent_job_id = completed_source_job(&store).await;

    // Sanity: the parent really is terminal before either child is tracked.
    let parent_before = store
        .get(parent_job_id)
        .await
        .expect("get parent job")
        .expect("parent job exists");
    assert_eq!(parent_before.status, LifecycleStatus::Completed);

    track_graph_mutation(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_graph_summary(false),
    )
    .await;
    track_prune(
        Some(store.clone()),
        parent_job_id,
        None,
        &nonempty_drain_summary(0),
    )
    .await;

    let graph_child = only_job(&store, JobKind::Graph).await;
    assert_eq!(graph_child.status, LifecycleStatus::Completed);
    assert_eq!(graph_child.parent_job_id, Some(parent_job_id));
    assert_eq!(graph_child.root_job_id, Some(parent_job_id));

    let prune_child = only_job(&store, JobKind::Prune).await;
    assert_eq!(prune_child.status, LifecycleStatus::Completed);
    assert_eq!(prune_child.parent_job_id, Some(parent_job_id));
    assert_eq!(prune_child.root_job_id, Some(parent_job_id));

    // The parent's own row is unaffected by children being created/completed
    // against it after its own terminal transition.
    let parent_after = store
        .get(parent_job_id)
        .await
        .expect("get parent job")
        .expect("parent job exists");
    assert_eq!(parent_after.status, LifecycleStatus::Completed);
    assert_eq!(parent_after.parent_job_id, None);
}
