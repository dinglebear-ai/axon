use super::*;
use axon_api::source::{ErrorStage, JobKind, TransportKind};

fn snapshot_with(scopes: Vec<AuthScope>) -> AuthSnapshot {
    AuthSnapshot {
        granted_scopes: scopes,
        ..AuthSnapshot::default()
    }
}

#[test]
fn write_scope_does_not_satisfy_admin_execute_or_local() {
    let snapshot = snapshot_with(vec![AuthScope::Read, AuthScope::Write]);
    assert!(require_job_scope(&snapshot, AuthScope::Admin).is_err());
    assert!(require_job_scope(&snapshot, AuthScope::Execute).is_err());
    assert!(require_job_scope(&snapshot, AuthScope::Local).is_err());
}

#[test]
fn require_job_scope_passes_when_scope_is_granted() {
    let snapshot = snapshot_with(vec![AuthScope::Read, AuthScope::Write, AuthScope::Local]);
    assert!(require_job_scope(&snapshot, AuthScope::Local).is_ok());
}

#[test]
fn execute_job_without_execute_scope_fails_before_side_effect() {
    let snapshot = snapshot_with(vec![AuthScope::Read, AuthScope::Write]);
    let error = require_job_scope(&snapshot, AuthScope::Execute).unwrap_err();
    assert_eq!(error.code.to_string(), "auth.scope_required");
    assert_eq!(error.stage, ErrorStage::Authorizing);
}

#[test]
fn child_job_inherits_parent_auth_snapshot() {
    let parent = AuthSnapshot {
        caller_id: Some("user_1".to_string()),
        transport: TransportKind::Mcp,
        granted_scopes: vec![AuthScope::Read, AuthScope::Write],
        ..AuthSnapshot::default()
    };
    let child = child_auth_snapshot(&parent);
    assert_eq!(child.granted_scopes, parent.granted_scopes);
    assert_eq!(child.caller_id, parent.caller_id);
    assert_eq!(child.transport, parent.transport);
}

#[test]
fn stale_reclaim_does_not_gain_new_local_scope() {
    // A job originally submitted without `axon:local` must not pass a later
    // local-scope check just because it went through retry/reclaim — the
    // snapshot recorded at enqueue time is the only thing that's ever
    // consulted, never current process/caller defaults.
    let reclaimed_snapshot = snapshot_with(vec![AuthScope::Read, AuthScope::Write]);
    let decision = require_job_scope(&reclaimed_snapshot, AuthScope::Local);
    let error = decision.unwrap_err();
    assert_eq!(error.code.to_string(), "auth.scope_required");
}

#[test]
fn reset_and_prune_require_admin_scope() {
    assert_eq!(
        required_scope_for_kind(JobKind::Reset),
        Some(AuthScope::Admin)
    );
    assert_eq!(
        required_scope_for_kind(JobKind::Prune),
        Some(AuthScope::Admin)
    );
}

#[test]
fn ordinary_job_kinds_require_no_additional_scope() {
    assert_eq!(required_scope_for_kind(JobKind::Source), None);
    assert_eq!(required_scope_for_kind(JobKind::Watch), None);
    assert_eq!(required_scope_for_kind(JobKind::Memory), None);
}
