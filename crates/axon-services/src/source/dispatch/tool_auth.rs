//! Server-owned authorization policy for executable tool sources.

use std::collections::BTreeMap;

use axon_adapters::{cli_tool::parse_cli_tool_source, mcp_tool::parse_mcp_target};
use axon_api::source::{AuthScope, AuthSnapshot, SourceKind, SourceScope};

use crate::source::tool_policy::{
    ToolExecutionAuditSnapshot, ToolSourceExecutionRequest, validate_tool_source_execution,
};

const POLICY_ID: &str = "source-tool-execution/v2";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_OUTPUT_CAP_BYTES: u64 = 64 * 1024;
const MAX_OUTPUT_CAP_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct ToolExecutionPolicy {
    enabled: bool,
    command_allowlist: Vec<String>,
    mcp_allowlist: Vec<String>,
    mcp_caller_command: Option<String>,
    mcp_caller_allowlist: Vec<String>,
    env_allowlist: Vec<String>,
    timeout_ms: u64,
    output_cap_bytes: u64,
}

#[derive(Debug, Clone)]
pub(super) struct AuthorizedToolExecution {
    pub(super) execute: bool,
    pub(super) policy_metadata: serde_json::Value,
    pub(super) policy_id: &'static str,
}

impl ToolExecutionPolicy {
    pub(crate) fn from_process() -> Self {
        Self {
            enabled: env_bool("AXON_ALLOW_TOOL_EXECUTION"),
            command_allowlist: env_list("AXON_TOOL_COMMAND_ALLOWLIST"),
            mcp_allowlist: env_list("AXON_MCP_TOOL_ALLOWLIST"),
            mcp_caller_command: std::env::var("AXON_MCP_CALLER_COMMAND")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            mcp_caller_allowlist: env_list("AXON_MCP_CALLER_ALLOWLIST"),
            env_allowlist: env_list("AXON_TOOL_ENV_ALLOWLIST"),
            timeout_ms: env_u64("AXON_TOOL_TIMEOUT_MS", DEFAULT_TIMEOUT_MS)
                .clamp(1, MAX_TIMEOUT_MS),
            output_cap_bytes: env_u64("AXON_MAX_TOOL_OUTPUT_BYTES", DEFAULT_OUTPUT_CAP_BYTES)
                .clamp(1, MAX_OUTPUT_CAP_BYTES),
        }
    }

    #[cfg(test)]
    pub(super) fn test_cli(command: &str) -> Self {
        Self {
            enabled: true,
            command_allowlist: vec![command.to_string()],
            mcp_allowlist: Vec::new(),
            mcp_caller_command: None,
            mcp_caller_allowlist: Vec::new(),
            env_allowlist: Vec::new(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            output_cap_bytes: DEFAULT_OUTPUT_CAP_BYTES,
        }
    }

    #[cfg(test)]
    pub(super) fn test_mcp(target: &str, caller: &str) -> Self {
        Self {
            enabled: true,
            command_allowlist: Vec::new(),
            mcp_allowlist: vec![target.to_string()],
            mcp_caller_command: Some(caller.to_string()),
            mcp_caller_allowlist: vec![caller.to_string()],
            env_allowlist: Vec::new(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            output_cap_bytes: DEFAULT_OUTPUT_CAP_BYTES,
        }
    }

    #[cfg(test)]
    pub(super) fn test_mcp_without_caller(target: &str) -> Self {
        let mut policy = Self::test_mcp(target, "/bin/echo");
        policy.mcp_caller_command = None;
        policy.mcp_caller_allowlist.clear();
        policy
    }
}

pub(super) fn authorize_cli_tool_execution(
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<AuthorizedToolExecution> {
    if !tool_execution_requested(route, &["execute", "exec", "run", "invoke"]) {
        return Ok(metadata_only());
    }
    require_tool_execution_scope(route, auth_snapshot, policy)?;
    let source = parse_cli_tool_source(input).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    validate_execution(
        SourceKind::CliTool,
        std::iter::once(source.command).chain(source.argv).collect(),
        &policy.command_allowlist,
        policy,
    )?;
    Ok(authorized(policy, None))
}

pub(super) fn authorize_mcp_tool_execution(
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<AuthorizedToolExecution> {
    if !tool_execution_requested(route, &["call", "invoke", "execute", "exec"]) {
        return Ok(metadata_only());
    }
    require_tool_execution_scope(route, auth_snapshot, policy)?;
    let target = parse_mcp_target(input).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    validate_execution(
        SourceKind::McpTool,
        vec![format!("{}/{}", target.server, target.tool)],
        &policy.mcp_allowlist,
        policy,
    )?;
    let caller = policy.mcp_caller_command.as_ref().ok_or_else(|| {
        anyhow::anyhow!("mcp.caller_missing: server MCP caller command is not configured")
    })?;
    validate_execution(
        SourceKind::CliTool,
        vec![caller.clone()],
        &policy.mcp_caller_allowlist,
        policy,
    )?;
    Ok(authorized(policy, Some(caller)))
}

fn validate_execution(
    source_kind: SourceKind,
    command: Vec<String>,
    allowlist: &[String],
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<()> {
    validate_tool_source_execution(&ToolSourceExecutionRequest {
        source_kind,
        execution_requested: true,
        command,
        env: BTreeMap::new(),
        timeout_ms: Some(policy.timeout_ms),
        output_cap_bytes: Some(policy.output_cap_bytes),
        audit_snapshot: Some(ToolExecutionAuditSnapshot {
            policy_id: POLICY_ID.to_string(),
            side_effect_class: "read".to_string(),
            command_allowlist: allowlist.to_vec(),
            env_allowlist: policy.env_allowlist.clone(),
            shell_expansion_allowed: false,
        }),
    })?;
    Ok(())
}

fn require_tool_execution_scope(
    route: &axon_api::source::RoutePlan,
    auth_snapshot: Option<&AuthSnapshot>,
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<()> {
    if !policy.enabled {
        anyhow::bail!("tool.execution_disabled: server policy disables tool execution");
    }
    if route.scope != SourceScope::Api {
        anyhow::bail!(
            "tool.scope_required: executable/callable tool sources require source scope api"
        );
    }
    if let Some(snapshot) = auth_snapshot
        && !crate::source::authorize::snapshot_allows_scope(snapshot, AuthScope::Execute)
    {
        anyhow::bail!("auth.scope_required: tool execution requires axon:execute");
    }
    Ok(())
}

fn authorized(policy: &ToolExecutionPolicy, caller: Option<&str>) -> AuthorizedToolExecution {
    AuthorizedToolExecution {
        execute: true,
        policy_id: POLICY_ID,
        policy_metadata: serde_json::json!({
            "policy_id": POLICY_ID,
            "command_allowlist": policy.command_allowlist.clone(),
            "mcp_allowlist": policy.mcp_allowlist.clone(),
            "mcp_caller_command": caller,
            "env_allowlist": policy.env_allowlist.clone(),
            "side_effect_class": "read",
            "timeout_ms": policy.timeout_ms,
            "output_cap_bytes": policy.output_cap_bytes,
        }),
    }
}

fn metadata_only() -> AuthorizedToolExecution {
    AuthorizedToolExecution {
        execute: false,
        policy_id: POLICY_ID,
        policy_metadata: serde_json::json!({"policy_id": POLICY_ID}),
    }
}

fn tool_execution_requested(route: &axon_api::source::RoutePlan, modes: &[&str]) -> bool {
    option_string_any(route, &["execution_mode", "tool_action"]).is_some_and(|mode| {
        modes
            .iter()
            .any(|allowed| mode.eq_ignore_ascii_case(allowed))
    }) || bool_option(route, "execute").unwrap_or(false)
        || bool_option(route, "call").unwrap_or(false)
}

fn option_string_any(route: &axon_api::source::RoutePlan, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        route
            .validated_options
            .values
            .0
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    })
}

fn bool_option(route: &axon_api::source::RoutePlan, key: &str) -> Option<bool> {
    route
        .validated_options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_bool)
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
