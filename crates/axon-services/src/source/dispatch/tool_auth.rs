//! Auth and policy checks for executable tool sources.

use axon_adapters::{cli_tool::parse_cli_tool_source, mcp_tool::parse_mcp_target};
use axon_api::source::{AuthScope, AuthSnapshot, SourceKind, SourceScope};

use crate::source::tool_policy::{
    ToolExecutionAuditSnapshot, ToolSourceExecutionRequest, validate_tool_source_execution,
};

pub(super) fn authorize_cli_tool_execution(
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<bool> {
    if !tool_execution_requested(route, &["execute", "exec", "run", "invoke"]) {
        return Ok(false);
    }
    require_tool_execution_scope(route, auth_snapshot)?;
    let source = parse_cli_tool_source(input).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    validate_tool_source_execution(&ToolSourceExecutionRequest {
        source_kind: SourceKind::CliTool,
        execution_requested: true,
        command: std::iter::once(source.command).chain(source.argv).collect(),
        env: std::collections::BTreeMap::new(),
        timeout_ms: Some(tool_timeout_ms(route)),
        output_cap_bytes: Some(tool_output_cap_bytes(route)),
        audit_snapshot: Some(tool_audit_snapshot(
            route,
            tool_allowlist(route, &["command_allowlist", "tool_allowlist"]),
        )),
    })?;
    Ok(true)
}

pub(super) fn authorize_mcp_tool_execution(
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<bool> {
    if !tool_execution_requested(route, &["call", "invoke", "execute", "exec"]) {
        return Ok(false);
    }
    require_tool_execution_scope(route, auth_snapshot)?;
    let target = parse_mcp_target(input).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let target_key = format!("{}/{}", target.server, target.tool);
    validate_tool_source_execution(&ToolSourceExecutionRequest {
        source_kind: SourceKind::McpTool,
        execution_requested: true,
        command: vec![target_key],
        env: std::collections::BTreeMap::new(),
        timeout_ms: Some(tool_timeout_ms(route)),
        output_cap_bytes: Some(tool_output_cap_bytes(route)),
        audit_snapshot: Some(tool_audit_snapshot(
            route,
            tool_allowlist(route, &["mcp_allowlist", "tool_allowlist"]),
        )),
    })?;

    let caller_command = string_option(route, "mcp_caller_command").ok_or_else(|| {
        anyhow::anyhow!("mcp.caller_missing: MCP call mode requires mcp_caller_command")
    })?;
    validate_tool_source_execution(&ToolSourceExecutionRequest {
        source_kind: SourceKind::CliTool,
        execution_requested: true,
        command: vec![caller_command],
        env: std::collections::BTreeMap::new(),
        timeout_ms: Some(tool_timeout_ms(route)),
        output_cap_bytes: Some(tool_output_cap_bytes(route)),
        audit_snapshot: Some(tool_audit_snapshot(
            route,
            tool_allowlist(route, &["mcp_caller_allowlist"]),
        )),
    })?;
    Ok(true)
}

fn require_tool_execution_scope(
    route: &axon_api::source::RoutePlan,
    auth_snapshot: Option<&AuthSnapshot>,
) -> anyhow::Result<()> {
    if route.scope != SourceScope::Api {
        return Err(anyhow::anyhow!(
            "tool.scope_required: executable/callable tool sources require source scope api"
        ));
    }
    let Some(snapshot) = auth_snapshot else {
        return Ok(());
    };
    if crate::source::authorize::snapshot_allows_scope(snapshot, AuthScope::Execute) {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "auth.scope_required: tool execution requires axon:execute"
    ))
}

fn tool_execution_requested(
    route: &axon_api::source::RoutePlan,
    executable_modes: &[&str],
) -> bool {
    option_string_any(route, &["execution_mode", "tool_action"]).is_some_and(|mode| {
        executable_modes
            .iter()
            .any(|allowed| mode.eq_ignore_ascii_case(allowed))
    }) || bool_option(route, "execute").unwrap_or(false)
        || bool_option(route, "call").unwrap_or(false)
}

fn tool_audit_snapshot(
    route: &axon_api::source::RoutePlan,
    command_allowlist: Vec<String>,
) -> ToolExecutionAuditSnapshot {
    ToolExecutionAuditSnapshot {
        policy_id: "source-tool-execution/v1".to_string(),
        side_effect_class: string_option(route, "side_effect_class")
            .unwrap_or_else(|| "none".to_string()),
        command_allowlist,
        env_allowlist: tool_allowlist(route, &["env_allowlist"]),
        shell_expansion_allowed: false,
    }
}

fn tool_allowlist(route: &axon_api::source::RoutePlan, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| string_list_option(route, key))
        .unwrap_or_default()
}

fn tool_timeout_ms(route: &axon_api::source::RoutePlan) -> u64 {
    u64_option(route, "timeout_ms").unwrap_or(5_000).max(1)
}

fn tool_output_cap_bytes(route: &axon_api::source::RoutePlan) -> u64 {
    u64_option(route, "output_cap_bytes")
        .unwrap_or(64 * 1024)
        .max(1)
}

fn option_string_any(route: &axon_api::source::RoutePlan, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_option(route, key))
}

fn string_option(route: &axon_api::source::RoutePlan, key: &str) -> Option<String> {
    route
        .validated_options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn string_list_option(route: &axon_api::source::RoutePlan, key: &str) -> Option<Vec<String>> {
    let value = route.validated_options.values.0.get(key)?;
    if let Some(values) = value.as_array() {
        return Some(
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect(),
        );
    }
    value.as_str().map(|single| vec![single.to_string()])
}

fn u64_option(route: &axon_api::source::RoutePlan, key: &str) -> Option<u64> {
    route
        .validated_options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_u64)
}

fn bool_option(route: &axon_api::source::RoutePlan, key: &str) -> Option<bool> {
    route
        .validated_options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_bool)
}
