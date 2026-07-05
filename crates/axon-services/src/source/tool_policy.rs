use std::collections::BTreeMap;

use axon_api::source::{ApiError, SourceKind};
use axon_error::ErrorStage;

pub struct ToolExecutionAuditSnapshot {
    pub policy_id: String,
    pub side_effect_class: String,
    pub command_allowlist: Vec<String>,
    pub env_allowlist: Vec<String>,
    pub shell_expansion_allowed: bool,
}

pub struct ToolSourceExecutionRequest {
    pub source_kind: SourceKind,
    pub execution_requested: bool,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub output_cap_bytes: Option<u64>,
    pub audit_snapshot: Option<ToolExecutionAuditSnapshot>,
}

pub fn validate_tool_source_execution(
    request: &ToolSourceExecutionRequest,
) -> Result<(), ApiError> {
    if !request.execution_requested {
        return Ok(());
    }

    let Some(snapshot) = request.audit_snapshot.as_ref() else {
        return Err(policy_error(
            "tool.execution_policy_missing",
            "trusted tool execution audit snapshot is required",
        ));
    };
    let Some(command) = request.command.first() else {
        return Err(policy_error(
            "tool.command_missing",
            "tool execution requires an argv command",
        ));
    };
    if !matches!(
        request.source_kind,
        SourceKind::CliTool | SourceKind::McpTool
    ) {
        return Err(policy_error(
            "tool.source_kind_invalid",
            "tool execution is only valid for CLI or MCP tool sources",
        ));
    }
    if !snapshot
        .command_allowlist
        .iter()
        .any(|allowed| allowed == command)
    {
        return Err(policy_error(
            "tool.command_not_allowlisted",
            "tool execution command is not allowlisted by trusted policy",
        ));
    }
    if !snapshot.shell_expansion_allowed && command_requires_shell_expansion(&request.command) {
        return Err(policy_error(
            "tool.shell_expansion_denied",
            "tool execution policy denies shell expansion",
        ));
    }
    for key in request.env.keys() {
        if !snapshot.env_allowlist.iter().any(|allowed| allowed == key) {
            return Err(policy_error(
                "tool.env_not_allowlisted",
                "tool execution environment contains a non-allowlisted key",
            ));
        }
    }
    if request.timeout_ms.unwrap_or(0) == 0 {
        return Err(policy_error(
            "tool.timeout_required",
            "tool execution requires a nonzero timeout",
        ));
    }
    if request.output_cap_bytes.unwrap_or(0) == 0 {
        return Err(policy_error(
            "tool.output_cap_required",
            "tool execution requires a nonzero output cap",
        ));
    }

    Ok(())
}

fn command_requires_shell_expansion(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|cmd| matches!(cmd.as_str(), "sh" | "bash" | "zsh" | "cmd" | "powershell"))
        || command.iter().any(|arg| arg == "-c" || arg == "/c")
}

fn policy_error(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(code, ErrorStage::Authorizing, message)
}

#[cfg(test)]
#[path = "tool_policy_tests.rs"]
mod tests;
