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
    let result =
        resolve_and_acquire("tool:rg --help", ToolExecutionMode::Execute, true, &["rg"]).unwrap();

    assert_eq!(result.execution_count, 1);
    assert_eq!(result.documents[0].redaction_status, "clean");
}
