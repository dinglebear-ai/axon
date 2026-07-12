use super::*;
use axon_services::source::classify::SourceInputKind;

#[test]
fn local_source_maps_to_local_filesystem_and_local_scope() {
    let class = safety_class_for(SourceInputKind::Local);
    assert_eq!(class, SafetyClass::LocalFilesystem);
    assert_eq!(required_scope_for(class), axon_authz::AXON_LOCAL_SCOPE);
}

#[test]
fn web_source_maps_to_public_network_and_write_scope() {
    let class = safety_class_for(SourceInputKind::Web);
    assert_eq!(class, SafetyClass::PublicNetwork);
    assert_eq!(required_scope_for(class), axon_authz::AXON_WRITE_SCOPE);
}

#[test]
fn git_and_registry_sources_are_network_class() {
    assert_eq!(
        safety_class_for(SourceInputKind::Git),
        SafetyClass::PublicNetwork
    );
    assert_eq!(
        safety_class_for(SourceInputKind::Registry),
        SafetyClass::PublicNetwork
    );
}

#[test]
fn tool_execution_class_requires_execute_scope() {
    assert_eq!(
        required_scope_for(SafetyClass::ToolExecution),
        axon_authz::AXON_EXECUTE_SCOPE
    );
}

fn auth_ctx(scopes: &[&str]) -> AuthContext {
    AuthContext {
        sub: "tester".to_string(),
        actor_key: None,
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        issuer: "test".to_string(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

#[tokio::test]
async fn write_only_caller_is_denied_a_local_source() {
    // A local path source classified as LocalFilesystem requires axon:local;
    // a caller holding only axon:write must be rejected with auth.forbidden.
    let request = SourceRequest::local_path("/etc", true);
    let auth = auth_ctx(&[axon_authz::AXON_WRITE_SCOPE]);
    let err = authorize_source_request(&request, &auth)
        .await
        .expect_err("write-only caller must be denied a local source");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn local_scoped_caller_is_allowed_a_local_source() {
    let request = SourceRequest::local_path("/etc", true);
    let auth = auth_ctx(&[axon_authz::AXON_LOCAL_SCOPE]);
    authorize_source_request(&request, &auth)
        .await
        .expect("axon:local caller must be allowed a local source");
}

#[tokio::test]
async fn write_caller_is_allowed_a_web_source() {
    let request = SourceRequest::new("https://example.com/docs");
    let auth = auth_ctx(&[axon_authz::AXON_WRITE_SCOPE]);
    authorize_source_request(&request, &auth)
        .await
        .expect("axon:write caller must be allowed a network source");
}
