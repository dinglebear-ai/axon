use axon_api::source::{
    AuthMode, CallerContext, ExecutionAffinity, SafetyClass, TransportKind, Visibility,
};

use super::*;

fn caller(trusted_local: bool, scopes: Vec<String>) -> CallerContext {
    CallerContext {
        caller_id: Some("tester".to_string()),
        transport: TransportKind::Cli,
        trusted_local,
        scopes,
        visibility_ceiling: Visibility::Public,
        auth_mode: AuthMode::Test,
        token_id: None,
        display_name: None,
    }
}

#[test]
fn required_scope_maps_by_safety_class() {
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::ToolExecution),
        crate::AXON_EXECUTE_SCOPE
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::LocalFilesystem),
        crate::AXON_LOCAL_SCOPE
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::PublicNetwork),
        crate::AXON_WRITE_SCOPE
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::AuthenticatedNetwork),
        crate::AXON_WRITE_SCOPE
    );
}

#[test]
fn worker_affinity_only_needs_scope_not_local_trust() {
    let policy = AffinityPolicy::new();
    let c = caller(false, vec![crate::AXON_LOCAL_SCOPE.to_string()]);
    let decision = policy.evaluate(&c, SafetyClass::LocalFilesystem, ExecutionAffinity::Worker);
    assert!(decision.allowed);
    assert!(decision.warnings.is_empty());
}

#[test]
fn inline_local_filesystem_requires_local_trust_even_with_scope() {
    let policy = AffinityPolicy::new();
    let c = caller(false, vec![crate::AXON_LOCAL_SCOPE.to_string()]);
    let decision = policy.evaluate(&c, SafetyClass::LocalFilesystem, ExecutionAffinity::Inline);
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "denied.affinity_requires_local_trust");
    assert_eq!(decision.warnings.len(), 1);
}

#[test]
fn inline_local_filesystem_allowed_for_trusted_local_caller() {
    let policy = AffinityPolicy::new();
    let c = caller(true, vec![crate::AXON_LOCAL_SCOPE.to_string()]);
    let decision = policy.evaluate(&c, SafetyClass::LocalFilesystem, ExecutionAffinity::Inline);
    assert!(decision.allowed);
}

#[test]
fn missing_scope_denies_regardless_of_trust() {
    let policy = AffinityPolicy::new();
    let c = caller(true, Vec::new());
    let decision = policy.evaluate(&c, SafetyClass::ToolExecution, ExecutionAffinity::Worker);
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "denied.scope_missing");
}

#[test]
fn network_source_scope_is_write() {
    let policy = AffinityPolicy::new();
    let c = caller(false, vec![crate::AXON_WRITE_SCOPE.to_string()]);
    let decision = policy.evaluate(
        &c,
        SafetyClass::PublicNetwork,
        ExecutionAffinity::ProviderBound,
    );
    assert!(decision.allowed);
    assert_eq!(decision.scope, crate::AXON_WRITE_SCOPE);
}
