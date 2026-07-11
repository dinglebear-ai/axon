use crate::cli_tool::{ToolExecutionMode, resolve_and_acquire};

#[tokio::test]
async fn cli_tool_defaults_to_metadata_only() {
    let result = resolve_and_acquire(
        "tool:rg --help",
        ToolExecutionMode::MetadataOnly,
        false,
        &[],
    )
    .unwrap();

    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.execution_count, 0);
    assert_eq!(result.source.command, "rg");
    assert_eq!(result.source.argv, ["--help"]);
    assert_eq!(result.source.side_effect_class, "none");

    assert_eq!(result.graph_nodes.len(), 1);
    assert_eq!(result.graph_nodes[0].node_kind, "cli_tool");
    assert_eq!(result.graph_nodes[0].stable_key, "rg");
    assert!(
        result
            .tool_facts
            .iter()
            .any(|fact| fact.name == "command" && fact.value == "rg")
    );
}

#[tokio::test]
async fn cli_tool_exec_requires_execute_scope_and_allowlist() {
    let err = resolve_and_acquire("tool:rg --help", ToolExecutionMode::Execute, false, &["rg"])
        .unwrap_err();
    assert_eq!(err.code, "auth.scope_required");

    let err = resolve_and_acquire("tool:sh -c env", ToolExecutionMode::Execute, true, &["rg"])
        .unwrap_err();
    assert_eq!(err.code, "tool.command_denied");
}

#[tokio::test]
async fn cli_tool_exec_records_one_execution_when_explicitly_allowed() {
    // `/bin/echo` is used (rather than `rg`) so this test does not depend on
    // an optional dev-tool being on `PATH` in every CI environment.
    let result = resolve_and_acquire(
        "tool:/bin/echo hello",
        ToolExecutionMode::Execute,
        true,
        &["/bin/echo"],
    )
    .unwrap();

    assert_eq!(result.execution_count, 1);
    assert_eq!(result.documents[0].redaction_status, "clean");
    assert_eq!(result.documents[0].exit_code, Some(0));
    assert!(result.documents[0].content.contains("hello"));
}

#[tokio::test]
async fn cli_tool_exec_redacts_secret_shaped_output() {
    let result = resolve_and_acquire(
        "tool:/bin/echo Authorization: Bearer sk-shhh",
        ToolExecutionMode::Execute,
        true,
        &["/bin/echo"],
    )
    .unwrap();

    assert_eq!(result.execution_count, 1);
    assert_eq!(result.documents[0].redaction_status, "redacted");
    assert!(!result.documents[0].content.contains("sk-shhh"));
}
