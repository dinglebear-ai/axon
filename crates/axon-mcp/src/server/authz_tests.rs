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
