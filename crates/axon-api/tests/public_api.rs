//! External-caller smoke for the pipeline DTO contract.

use axon_api::source::{
    AuthScope, AuthSnapshot, JobPriority, SourceRequest, SourceScope, TransportKind, Visibility,
};

#[test]
fn canonical_source_request_is_constructible_by_external_callers() {
    let mut request = SourceRequest::new("https://example.com/docs");
    request.scope = Some(SourceScope::Site);
    request.execution.priority = JobPriority::High;
    assert_eq!(request.scope, Some(SourceScope::Site));
    assert_eq!(request.execution.priority, JobPriority::High);
}

#[test]
fn panel_snapshot_is_fixed_and_cannot_mint_local_execute_or_admin() {
    let snapshot = AuthSnapshot::panel("test-policy");
    assert_eq!(snapshot.transport, TransportKind::Rest);
    assert_eq!(snapshot.visibility_ceiling, Visibility::Internal);
    assert_eq!(
        snapshot.granted_scopes,
        vec![AuthScope::Read, AuthScope::Write]
    );
    assert!(!snapshot.granted_scopes.contains(&AuthScope::Admin));
    assert!(!snapshot.granted_scopes.contains(&AuthScope::Local));
    assert!(!snapshot.granted_scopes.contains(&AuthScope::Execute));
}
