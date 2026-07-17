use super::*;

#[test]
fn reset_is_admin_only_and_rejects_unknown_subactions() {
    assert_eq!(required_scope_for("reset", "plan"), Some("axon:admin"));
    assert_eq!(required_scope_for("reset", "exec"), Some("axon:admin"));
    assert_eq!(required_scope_for("reset", "purge"), Some("__deny__"));
}

#[test]
fn collections_is_read_only_and_rejects_mutation_subactions() {
    assert_eq!(required_scope_for("collections", "list"), Some("axon:read"));
    assert_eq!(required_scope_for("collections", "get"), Some("axon:read"));
    assert_eq!(
        required_scope_for("collections", "delete"),
        Some("__deny__")
    );
}

#[test]
fn uploads_split_read_and_write_scopes_and_reject_unknown_subactions() {
    for subaction in ["list", "get"] {
        assert_eq!(required_scope_for("uploads", subaction), Some("axon:read"));
    }
    for subaction in ["create", "put_content", "complete", "abort"] {
        assert_eq!(required_scope_for("uploads", subaction), Some("axon:write"));
    }
    assert_eq!(required_scope_for("uploads", "delete"), Some("__deny__"));
}

#[test]
fn artifacts_and_chat_are_read_scoped() {
    for subaction in ["list", "get", "content"] {
        assert_eq!(
            required_scope_for("artifacts", subaction),
            Some("axon:read")
        );
    }
    assert_eq!(required_scope_for("artifacts", "delete"), Some("__deny__"));
    assert_eq!(required_scope_for("chat", ""), Some("axon:read"));
    assert_eq!(required_scope_for("chat", "stream"), Some("__deny__"));
}
