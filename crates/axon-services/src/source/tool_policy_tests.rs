use std::collections::BTreeMap;

use axon_api::source::SourceKind;

use super::{
    ToolExecutionAuditSnapshot, ToolSourceExecutionRequest, validate_tool_source_execution,
};

#[test]
fn tool_source_execution_requires_trusted_policy_snapshot() {
    let request = ToolSourceExecutionRequest {
        source_kind: SourceKind::CliTool,
        execution_requested: true,
        command: vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
        env: BTreeMap::from([("OPENAI_API_KEY".to_string(), "sk-proj-secret".to_string())]),
        timeout_ms: None,
        output_cap_bytes: None,
        audit_snapshot: None,
    };

    let err =
        validate_tool_source_execution(&request).expect_err("trusted audit snapshot is required");
    assert_eq!(err.code.to_string(), "tool.execution_policy_missing");
}

#[test]
fn tool_source_execution_accepts_explicit_no_shell_allowlisted_policy() {
    let request = ToolSourceExecutionRequest {
        source_kind: SourceKind::McpTool,
        execution_requested: true,
        command: vec!["mcp-call".to_string(), "server.tool".to_string()],
        env: BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string())]),
        timeout_ms: Some(30_000),
        output_cap_bytes: Some(64 * 1024),
        audit_snapshot: Some(ToolExecutionAuditSnapshot {
            policy_id: "policy_tool_read".to_string(),
            side_effect_class: "read".to_string(),
            command_allowlist: vec!["mcp-call".to_string()],
            env_allowlist: vec!["PATH".to_string()],
            shell_expansion_allowed: false,
        }),
    };

    validate_tool_source_execution(&request).expect("trusted policy accepted");
}
