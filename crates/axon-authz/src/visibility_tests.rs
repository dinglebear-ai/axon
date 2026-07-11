use axon_api::source::{AuthMode, CallerContext, TransportKind, Visibility};

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
fn trusted_local_caller_gets_internal_ceiling() {
    let policy = VisibilityPolicy::new();
    let ceiling = policy.ceiling_for(&caller(true, Vec::new()));
    assert_eq!(ceiling, Visibility::Internal);
}

#[test]
fn admin_scoped_caller_gets_internal_ceiling() {
    let policy = VisibilityPolicy::new();
    let ceiling = policy.ceiling_for(&caller(false, vec![crate::AXON_ADMIN_SCOPE.to_string()]));
    assert_eq!(ceiling, Visibility::Internal);
}

#[test]
fn untrusted_non_admin_caller_gets_public_ceiling() {
    let policy = VisibilityPolicy::new();
    let ceiling = policy.ceiling_for(&caller(false, vec![crate::AXON_READ_SCOPE.to_string()]));
    assert_eq!(ceiling, Visibility::Public);
}

#[test]
fn free_function_matches_default_policy() {
    let c = caller(true, Vec::new());
    assert_eq!(ceiling_for(&c), VisibilityPolicy::new().ceiling_for(&c));
}

#[test]
fn redacted_is_always_visible() {
    let policy = VisibilityPolicy::new();
    assert!(policy.is_visible(Visibility::Redacted, Visibility::Public));
}

#[test]
fn public_ceiling_hides_internal_and_sensitive_fields() {
    let policy = VisibilityPolicy::new();
    assert!(policy.is_visible(Visibility::Public, Visibility::Public));
    assert!(!policy.is_visible(Visibility::Internal, Visibility::Public));
    assert!(!policy.is_visible(Visibility::Sensitive, Visibility::Public));
}

#[test]
fn internal_ceiling_reveals_internal_but_never_sensitive() {
    let policy = VisibilityPolicy::new();
    assert!(policy.is_visible(Visibility::Internal, Visibility::Internal));
    assert!(policy.is_visible(Visibility::Derived, Visibility::Internal));
    assert!(!policy.is_visible(Visibility::Sensitive, Visibility::Internal));
}
