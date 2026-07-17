use super::*;

#[test]
fn reset_exec_requires_only_canonical_wire_fields_at_parse_time() {
    let request: McpSystemRequest = serde_json::from_value(serde_json::json!({
        "action": "reset",
        "subaction": "exec",
        "plan_id": "reset_plan_123",
        "confirm": true
    }))
    .expect("canonical reset request parses");
    let McpSystemRequest::Reset(request) = request else {
        panic!("expected reset request");
    };
    assert!(matches!(request.subaction, Some(ResetSubaction::Exec)));
    assert_eq!(request.plan_id.as_deref(), Some("reset_plan_123"));
    assert_eq!(request.confirm, Some(true));
}

#[test]
fn collections_rejects_unsupported_delete_subaction() {
    let error = serde_json::from_value::<McpSystemRequest>(serde_json::json!({
        "action": "collections",
        "subaction": "delete",
        "collection": "axon"
    }))
    .expect_err("unsupported collection mutation must fail closed");
    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn uploads_request_is_strict_and_keeps_staging_identity_explicit() {
    let request: McpSystemRequest = serde_json::from_value(serde_json::json!({
        "action": "uploads",
        "subaction": "complete",
        "upload_id": "upl_abc",
        "sha256": "a".repeat(64)
    }))
    .unwrap();
    let McpSystemRequest::Uploads(request) = request else {
        panic!("expected uploads request")
    };
    assert_eq!(request.upload_id.as_deref(), Some("upl_abc"));
    assert!(
        serde_json::from_value::<McpSystemRequest>(serde_json::json!({
            "action": "uploads",
            "subaction": "get",
            "upload_id": "upl_abc",
        "artifact_id": "art_not_allowed"
        }))
        .is_err()
    );
}

#[test]
fn artifacts_request_uses_only_opaque_artifact_identity() {
    let request: McpSystemRequest = serde_json::from_value(serde_json::json!({
        "action": "artifacts",
        "subaction": "content",
        "artifact_id": "art_report_abc"
    }))
    .expect("canonical artifact request parses");
    let McpSystemRequest::Artifacts(request) = request else {
        panic!("expected artifacts request")
    };
    assert_eq!(request.artifact_id.as_deref(), Some("art_report_abc"));
    assert!(
        serde_json::from_value::<McpSystemRequest>(serde_json::json!({
            "action": "artifacts",
            "subaction": "content",
            "path": "screenshots/secret.png"
        }))
        .is_err()
    );
}

#[test]
fn watch_accepts_canonical_cursor_and_status_fields() {
    let request: McpWatchRequest = serde_json::from_value(serde_json::json!({
        "action": "watch",
        "subaction": "history",
        "id": "watch-1",
        "cursor": "opaque-cursor",
        "status": "failed"
    }))
    .expect("canonical watch cursor/filter request parses");
    let McpWatchRequest::Watch(request) = request;
    assert_eq!(request.cursor.as_deref(), Some("opaque-cursor"));
    assert_eq!(request.status, Some(LifecycleStatus::Failed));
}
