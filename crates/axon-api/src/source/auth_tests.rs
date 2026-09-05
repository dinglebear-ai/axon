use super::*;

#[test]
fn snapshot_visibility_cannot_exceed_caller_ceiling() {
    let caller = CallerContext {
        visibility_ceiling: Visibility::Public,
        ..CallerContext::default()
    };
    let snapshot = AuthSnapshot::from_caller(&caller, Visibility::Internal, "test");
    assert_eq!(snapshot.visibility_ceiling, Visibility::Public);
}

#[test]
fn snapshot_preserves_narrower_requested_visibility() {
    let caller = CallerContext {
        visibility_ceiling: Visibility::Internal,
        ..CallerContext::default()
    };
    let snapshot = AuthSnapshot::from_caller(&caller, Visibility::Public, "test");
    assert_eq!(snapshot.visibility_ceiling, Visibility::Public);
}
