use axon_api::source::{
    AuthMode, CallerContext, ExecutionAffinity, SafetyClass, SecurityPolicyRequest, TransportKind,
    Visibility,
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

#[tokio::test]
async fn scope_policy_evaluator_delegates_source_authorization_to_scope_policy() {
    let evaluator = ScopePolicyEvaluator::new(crate::AXON_READ_SCOPE);
    let decision = evaluator
        .authorize_source(SecurityPolicyRequest {
            caller: caller(true, vec![crate::AXON_READ_SCOPE.to_string()]),
            safety_class: SafetyClass::LocalFilesystem,
            target: "file:///repo".to_string(),
        })
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, "authorized.scope_satisfied");
}

#[tokio::test]
async fn scope_policy_evaluator_denies_missing_scope() {
    let evaluator = ScopePolicyEvaluator::new(crate::AXON_ADMIN_SCOPE);
    let decision = evaluator
        .authorize_source(SecurityPolicyRequest {
            caller: caller(false, vec![crate::AXON_WRITE_SCOPE.to_string()]),
            safety_class: SafetyClass::PublicNetwork,
            target: "https://example.com".to_string(),
        })
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "denied.scope_missing");
}

#[test]
fn scope_policy_evaluator_derives_visibility_ceiling_from_caller() {
    let evaluator = ScopePolicyEvaluator::new(crate::AXON_READ_SCOPE);
    let trusted = evaluator.visibility_ceiling(&caller(true, Vec::new()));
    let untrusted = evaluator.visibility_ceiling(&caller(false, Vec::new()));
    assert_eq!(trusted, Visibility::Internal);
    assert_eq!(untrusted, Visibility::Public);
}

#[test]
fn scope_policy_evaluator_execution_affinity_matches_affinity_policy() {
    let evaluator = ScopePolicyEvaluator::new(crate::AXON_READ_SCOPE);
    let c = caller(false, vec![crate::AXON_LOCAL_SCOPE.to_string()]);
    let decision =
        evaluator.execution_affinity(&c, SafetyClass::LocalFilesystem, ExecutionAffinity::Inline);
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "denied.affinity_requires_local_trust");
}

#[tokio::test]
async fn fake_policy_evaluator_allow_all_always_allows() {
    let evaluator = FakePolicyEvaluator::allow_all();
    let decision = evaluator
        .authorize_source(SecurityPolicyRequest {
            caller: caller(false, Vec::new()),
            safety_class: SafetyClass::ToolExecution,
            target: "cli://noop".to_string(),
        })
        .await
        .unwrap();
    assert!(decision.allowed);
}

#[tokio::test]
async fn fake_policy_evaluator_deny_all_always_denies() {
    let evaluator = FakePolicyEvaluator::deny_all();
    let decision = evaluator
        .authorize_source(SecurityPolicyRequest {
            caller: caller(true, vec![crate::AXON_ADMIN_SCOPE.to_string()]),
            safety_class: SafetyClass::ToolExecution,
            target: "cli://noop".to_string(),
        })
        .await
        .unwrap();
    assert!(!decision.allowed);
}

#[tokio::test]
async fn fake_policy_evaluator_degrade_allows_with_warning() {
    let evaluator = FakePolicyEvaluator::degrade();
    let decision = evaluator
        .authorize_source(SecurityPolicyRequest {
            caller: caller(true, Vec::new()),
            safety_class: SafetyClass::PublicNetwork,
            target: "https://example.com".to_string(),
        })
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.warnings.len(), 1);
}

#[test]
fn fake_policy_evaluator_ceiling_is_configurable() {
    let evaluator = FakePolicyEvaluator::allow_all().with_ceiling(Visibility::Internal);
    assert_eq!(
        evaluator.visibility_ceiling(&caller(false, Vec::new())),
        Visibility::Internal
    );
}
