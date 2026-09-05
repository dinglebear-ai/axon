use super::*;

fn caller_with_ceiling(visibility_ceiling: Visibility) -> CallerContext {
    CallerContext {
        caller_id: Some("test-caller".to_string()),
        transport: TransportKind::Rest,
        trusted_local: false,
        scopes: vec!["axon:read".to_string()],
        visibility_ceiling,
        auth_mode: AuthMode::Test,
        token_id: None,
        display_name: None,
    }
}

#[test]
fn snapshot_visibility_cannot_exceed_caller_ceiling() {
    let caller = caller_with_ceiling(Visibility::Public);
    let snapshot = AuthSnapshot::from_caller(&caller, Visibility::Internal, "test");
    assert_eq!(snapshot.visibility_ceiling, Visibility::Public);
}

#[test]
fn snapshot_preserves_narrower_requested_visibility() {
    let caller = caller_with_ceiling(Visibility::Internal);
    let snapshot = AuthSnapshot::from_caller(&caller, Visibility::Public, "test");
    assert_eq!(snapshot.visibility_ceiling, Visibility::Public);
}
