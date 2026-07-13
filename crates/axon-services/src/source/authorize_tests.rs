use super::*;
use crate::source::routing;
use axon_api::source::{AuthScope, AuthSnapshot, SafetyClass, SourceRequest, SourceScope};

/// The router's declared reddit credential requirement (see
/// `axon-route`'s `AdapterRegistry::target_defaults`) must survive into the
/// `RoutePlan` handed to `authorize_route` — this is the "adapter-declared
/// credential requirement appears in the routed plan" contract check.
#[test]
fn reddit_route_carries_declared_credential_requirement() {
    let mut request = SourceRequest::new("r/rust");
    request.scope = Some(SourceScope::Subreddit);

    let routed = routing::resolve_source_route(&request).expect("reddit source should route");

    assert_eq!(routed.route.adapter.name, "reddit");
    assert!(
        routed
            .route
            .credential_requirements
            .iter()
            .any(|req| req.required
                && req.credential_kind == axon_api::source::CredentialKind::ApiKey),
        "expected reddit route to declare a required ApiKey credential, got: {:?}",
        routed.route.credential_requirements
    );
}

/// Exercises the actual deny path of `authorize_route`. Rather than mutating
/// process-global env vars (flaky under parallel test execution), this
/// relies on `credential_present_in_env`'s fail-closed default: an adapter
/// name with no known env mapping is never treated as authorized, so a
/// required, not-pre-resolved credential on such an adapter deterministically
/// denies regardless of ambient env state.
#[test]
fn authorize_route_denies_when_required_credential_has_no_known_mapping() {
    let mut request = SourceRequest::new("r/rust");
    request.scope = Some(SourceScope::Subreddit);
    let mut routed = routing::resolve_source_route(&request).expect("reddit source should route");

    // Confirm the router actually declared a required, unresolved
    // credential requirement before we mutate the adapter name below.
    let requirement = routed
        .route
        .credential_requirements
        .first()
        .expect("reddit route must declare at least one credential requirement");
    assert!(requirement.required);
    assert!(requirement.secret_ref.is_none());

    // Swap in an adapter name `credential_present_in_env` doesn't recognize
    // to deterministically hit the fail-closed default arm, independent of
    // whatever REDDIT_CLIENT_ID/SECRET happen to be set in this process.
    routed.route.adapter.name = "unmapped-test-adapter".to_string();

    let result = authorize_route(&routed.route);
    assert!(
        result.is_err(),
        "expected authorize_route to deny a required credential with no known env mapping"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code.to_string(), "auth.credential_missing");
}

/// A `secret_ref`-resolved credential requirement is treated as already
/// satisfied and never consults `credential_present_in_env` — this holds
/// even for an adapter name with no known env mapping.
#[test]
fn authorize_route_allows_pre_resolved_credential_regardless_of_env_mapping() {
    let mut request = SourceRequest::new("r/rust");
    request.scope = Some(SourceScope::Subreddit);
    let mut routed = routing::resolve_source_route(&request).expect("reddit source should route");

    routed.route.adapter.name = "unmapped-test-adapter".to_string();
    for requirement in &mut routed.route.credential_requirements {
        requirement.secret_ref = Some(axon_api::source::SecretRef {
            provider: "test".to_string(),
            key: "test-key".to_string(),
            label: "test".to_string(),
        });
    }

    assert!(authorize_route(&routed.route).is_ok());
}

#[test]
fn authorize_route_allows_sources_without_credential_requirements() {
    let mut request = SourceRequest::new("example.com");
    request.scope = Some(SourceScope::Map);
    let routed = routing::resolve_source_route(&request).expect("web source should route");

    assert!(routed.route.credential_requirements.is_empty());
    assert!(authorize_route(&routed.route).is_ok());
}

#[test]
fn authorize_safety_class_denies_local_without_local_scope() {
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

    let err = authorize_safety_class(SafetyClass::LocalFilesystem, Some(&snapshot))
        .expect_err("local source requires explicit local scope");

    assert_eq!(err.code.to_string(), "auth.scope_required");
    assert_eq!(
        err.details.get("required_scope").map(String::as_str),
        Some("axon:local")
    );
}

#[test]
fn authorize_safety_class_allows_local_with_local_scope() {
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Local];

    authorize_safety_class(SafetyClass::LocalFilesystem, Some(&snapshot))
        .expect("local scope should authorize local source execution");
}

#[test]
fn authorize_safety_class_allows_trusted_local_none_snapshot() {
    authorize_safety_class(SafetyClass::LocalFilesystem, None)
        .expect("trusted local/loopback callers use the absence of a snapshot");
}

#[test]
fn authorize_safety_class_allows_trusted_system_snapshot() {
    let snapshot = AuthSnapshot::trusted_system("test");

    authorize_safety_class(SafetyClass::LocalFilesystem, Some(&snapshot))
        .expect("trusted local persisted snapshots authorize local source execution");
}

#[test]
fn authorize_safety_class_uses_route_safety_for_execute_sources() {
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

    let err = authorize_safety_class(SafetyClass::ToolExecution, Some(&snapshot))
        .expect_err("tool-execution route safety requires explicit execute scope");

    assert_eq!(err.code.to_string(), "auth.scope_required");
    assert_eq!(
        err.details.get("required_scope").map(String::as_str),
        Some("axon:execute")
    );
}
