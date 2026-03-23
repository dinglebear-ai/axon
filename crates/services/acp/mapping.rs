//! ACP SDK type mapping: converts `agent_client_protocol` types into our
//! service-layer representation (`AcpBridgeEvent`, `AcpSessionUpdateEvent`, etc.).

use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::{
    AcpAvailableCommand, AcpBridgeEvent, AcpCommandsUpdate, AcpConfigOption, AcpConfigSelectValue,
    AcpMcpServerConfig, AcpModeUpdate, AcpPermissionRequestEvent, AcpPlanEntry, AcpPlanUpdate,
    AcpSessionUpdateEvent, AcpSessionUpdateKind, AcpUsageUpdate,
};
use agent_client_protocol::{
    ContentBlock, EnvVariable, LoadSessionRequest, McpServer, McpServerHttp, McpServerStdio,
    NewSessionRequest, SessionConfigKind, SessionConfigOption as SdkConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOptions, SessionId, SessionNotification,
    SessionUpdate, ToolCallContent,
};
use std::error::Error;
use std::path::Path;

use super::AcpSessionSetupRequest;

// ── Public mapping functions ────────────────────────────────────────────────

pub fn map_session_update_kind(update: &SessionUpdate) -> AcpSessionUpdateKind {
    match update {
        SessionUpdate::UserMessageChunk(_) => AcpSessionUpdateKind::UserDelta,
        SessionUpdate::AgentMessageChunk(_) => AcpSessionUpdateKind::AssistantDelta,
        SessionUpdate::AgentThoughtChunk(_) => AcpSessionUpdateKind::ThinkingDelta,
        SessionUpdate::ToolCall(_) => AcpSessionUpdateKind::ToolCallStarted,
        SessionUpdate::ToolCallUpdate(_) => AcpSessionUpdateKind::ToolCallUpdated,
        SessionUpdate::Plan(_) => AcpSessionUpdateKind::Plan,
        SessionUpdate::AvailableCommandsUpdate(_) => AcpSessionUpdateKind::AvailableCommandsUpdate,
        SessionUpdate::CurrentModeUpdate(_) => AcpSessionUpdateKind::CurrentModeUpdate,
        SessionUpdate::ConfigOptionUpdate(_) => AcpSessionUpdateKind::ConfigOptionUpdate,
        SessionUpdate::UsageUpdate(_) => AcpSessionUpdateKind::UsageUpdate,
        _ => AcpSessionUpdateKind::Unknown,
    }
}

pub fn map_session_notification(notification: &SessionNotification) -> AcpSessionUpdateEvent {
    let kind = map_session_update_kind(&notification.update);
    let text_delta = extract_text_delta(&notification.update);
    let tool_call_id = extract_tool_call_id(&notification.update);
    let (tool_name, tool_status) = extract_tool_details(&notification.update);
    let tool_content = extract_tool_content(&notification.update);
    let tool_input = extract_tool_input(&notification.update);
    let tool_locations = extract_tool_locations(&notification.update);
    AcpSessionUpdateEvent {
        session_id: notification.session_id.0.to_string(),
        kind,
        text_delta,
        tool_call_id,
        tool_name,
        tool_status,
        tool_content,
        tool_input,
        tool_locations,
    }
}

pub fn map_permission_request(
    req: &agent_client_protocol::RequestPermissionRequest,
) -> AcpPermissionRequestEvent {
    let option_ids = req
        .options
        .iter()
        .map(|opt| opt.option_id.0.to_string())
        .collect::<Vec<_>>();
    AcpPermissionRequestEvent {
        session_id: req.session_id.0.to_string(),
        tool_call_id: req.tool_call.tool_call_id.0.to_string(),
        option_ids,
    }
}

pub fn map_permission_request_event(
    req: &agent_client_protocol::RequestPermissionRequest,
) -> ServiceEvent {
    ServiceEvent::AcpBridge {
        event: AcpBridgeEvent::PermissionRequest(map_permission_request(req)),
    }
}

/// Convert ACP SDK config options into our service-layer representation.
pub fn map_config_options(options: &[SdkConfigOption]) -> Vec<AcpConfigOption> {
    options
        .iter()
        .filter_map(|opt| {
            let select = match &opt.kind {
                SessionConfigKind::Select(select) => select,
                _ => return None,
            };
            let values = match &select.options {
                SessionConfigSelectOptions::Ungrouped(opts) => opts
                    .iter()
                    .map(|o| AcpConfigSelectValue {
                        value: o.value.0.to_string(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                    })
                    .collect(),
                SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .flat_map(|g| &g.options)
                    .map(|o| AcpConfigSelectValue {
                        value: o.value.0.to_string(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let category = opt.category.as_ref().map(|c| match c {
                SessionConfigOptionCategory::Mode => "mode".to_string(),
                SessionConfigOptionCategory::Model => "model".to_string(),
                SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
                SessionConfigOptionCategory::Other(s) => s.clone(),
                _ => "other".to_string(),
            });
            // Validate that current_value is actually present in the options list.
            // An absent or stale current_value violates the contract and could cause
            // the frontend to render a selected item that doesn't exist in the menu.
            // Drop the entire option entry rather than forwarding an invalid state.
            let current_value_str = select.current_value.0.to_string();
            if !select_options_contains_value(&select.options, &current_value_str) {
                tracing::warn!(
                    option_id = %opt.id.0,
                    current_value = %current_value_str,
                    "ACP config option current_value not found in options list; skipping option"
                );
                return None;
            }
            Some(AcpConfigOption {
                id: opt.id.0.to_string(),
                name: opt.name.clone(),
                description: opt.description.clone(),
                category,
                current_value: current_value_str,
                options: values,
            })
        })
        .collect()
}

pub(super) fn select_options_contains_value(
    options: &SessionConfigSelectOptions,
    requested: &str,
) -> bool {
    match options {
        SessionConfigSelectOptions::Ungrouped(values) => {
            values.iter().any(|v| v.value.0.as_ref() == requested)
        }
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|g| g.options.iter())
            .any(|v| v.value.0.as_ref() == requested),
        _ => false,
    }
}

pub fn map_session_notification_event(notification: &SessionNotification) -> ServiceEvent {
    let sid = notification.session_id.0.to_string();
    match &notification.update {
        SessionUpdate::ConfigOptionUpdate(update) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ConfigOptionsUpdate {
                session_id: sid,
                config_options: map_config_options(&update.config_options),
            },
        },
        SessionUpdate::Plan(plan) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::PlanUpdate(AcpPlanUpdate {
                session_id: sid,
                entries: plan
                    .entries
                    .iter()
                    .map(|e| AcpPlanEntry {
                        content: e.content.clone(),
                        priority: map_plan_priority(e.priority.clone()).to_string(),
                        status: map_plan_status(e.status.clone()).to_string(),
                    })
                    .collect(),
            }),
        },
        SessionUpdate::CurrentModeUpdate(mode) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ModeUpdate(AcpModeUpdate {
                session_id: sid,
                current_mode_id: mode.current_mode_id.0.to_string(),
            }),
        },
        SessionUpdate::AvailableCommandsUpdate(cmds) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::CommandsUpdate(AcpCommandsUpdate {
                session_id: sid,
                commands: cmds
                    .available_commands
                    .iter()
                    .map(|c| AcpAvailableCommand {
                        name: c.name.clone(),
                        description: Some(c.description.clone()),
                    })
                    .collect(),
            }),
        },
        SessionUpdate::UsageUpdate(usage) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::UsageUpdate(AcpUsageUpdate {
                session_id: sid,
                used: usage.used,
                size: usage.size,
                cost_amount: usage.cost.as_ref().map(|c| c.amount.to_string()),
                cost_currency: usage.cost.as_ref().map(|c| c.currency.clone()),
            }),
        },
        SessionUpdate::SessionInfoUpdate(_) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::SessionInfoUpdate { session_id: sid },
        },
        _ => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::SessionUpdate(map_session_notification(notification)),
        },
    }
}

// ── Private extraction helpers ──────────────────────────────────────────────

fn extract_tool_call_id(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(tool_call) => Some(tool_call.tool_call_id.0.to_string()),
        SessionUpdate::ToolCallUpdate(tool_call_update) => {
            Some(tool_call_update.tool_call_id.0.to_string())
        }
        _ => None,
    }
}

fn extract_tool_details(update: &SessionUpdate) -> (Option<String>, Option<String>) {
    match update {
        SessionUpdate::ToolCall(tool_call) => (
            Some(tool_call.title.clone()),
            Some(map_tool_status(tool_call.status).to_string()),
        ),
        SessionUpdate::ToolCallUpdate(tool_call_update) => {
            let title = tool_call_update.fields.title.clone();
            let status = tool_call_update
                .fields
                .status
                .map(|s| map_tool_status(s).to_string());
            (title, status)
        }
        _ => (None, None),
    }
}

fn map_tool_status(status: agent_client_protocol::ToolCallStatus) -> &'static str {
    use agent_client_protocol::ToolCallStatus;
    match status {
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "unknown",
    }
}

fn map_plan_priority(priority: agent_client_protocol::PlanEntryPriority) -> &'static str {
    use agent_client_protocol::PlanEntryPriority;
    match priority {
        PlanEntryPriority::Low => "low",
        PlanEntryPriority::Medium => "medium",
        PlanEntryPriority::High => "high",
        _ => "unknown",
    }
}

fn map_plan_status(status: agent_client_protocol::PlanEntryStatus) -> &'static str {
    use agent_client_protocol::PlanEntryStatus;
    match status {
        PlanEntryStatus::Pending => "pending",
        PlanEntryStatus::InProgress => "in_progress",
        PlanEntryStatus::Completed => "completed",
        _ => "unknown",
    }
}

fn extract_tool_locations(update: &SessionUpdate) -> Option<Vec<String>> {
    match update {
        SessionUpdate::ToolCall(tc) => Some(
            tc.locations
                .iter()
                .map(|l| l.path.to_string_lossy().to_string())
                .collect::<Vec<String>>(),
        )
        .filter(|v| !v.is_empty()),
        SessionUpdate::ToolCallUpdate(tcu) => tcu
            .fields
            .locations
            .as_ref()
            .map(|locs| {
                locs.iter()
                    .map(|l| l.path.to_string_lossy().to_string())
                    .collect::<Vec<String>>()
            })
            .filter(|v| !v.is_empty()),
        _ => None,
    }
}

pub(super) fn extract_text_delta(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => extract_content_text(&chunk.content),
        _ => None,
    }
}

fn extract_content_text(content: &ContentBlock) -> Option<String> {
    match content {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

fn extract_tool_content(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(tc) => tc.content.iter().find_map(|c| match c {
            ToolCallContent::Content(content) => extract_content_text(&content.content),
            _ => None,
        }),
        SessionUpdate::ToolCallUpdate(tcu) => tcu.fields.content.as_ref().and_then(|contents| {
            contents.iter().find_map(|c| match c {
                ToolCallContent::Content(content) => extract_content_text(&content.content),
                _ => None,
            })
        }),
        _ => None,
    }
}

fn extract_tool_input(update: &SessionUpdate) -> Option<serde_json::Value> {
    match update {
        SessionUpdate::ToolCall(tc) => tc.raw_input.clone(),
        SessionUpdate::ToolCallUpdate(tcu) => tcu.fields.raw_input.clone(),
        _ => None,
    }
}

// ── Validation (extracted to mapping/validation.rs) ─────────────────────────

mod validation;
pub use validation::{
    validate_adapter_command, validate_probe_request, validate_prompt_turn_request,
    validate_session_cwd,
};

// ── Session setup builder ───────────────────────────────────────────────────

pub(super) fn convert_mcp_servers(configs: &[AcpMcpServerConfig]) -> Vec<McpServer> {
    configs
        .iter()
        .map(|cfg| match cfg {
            AcpMcpServerConfig::Stdio {
                name,
                command,
                args,
                env,
            } => {
                let mut server = McpServerStdio::new(name.clone(), command.clone());
                if !args.is_empty() {
                    server = server.args(args.clone());
                }
                if !env.is_empty() {
                    server = server.env(
                        env.iter()
                            .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Stdio(server)
            }
            AcpMcpServerConfig::Http { name, url, .. } => {
                McpServer::Http(McpServerHttp::new(name.clone(), url.clone()))
            }
            AcpMcpServerConfig::Sse { name, url, .. } => {
                // TODO Task 2: full SSE support with headers
                McpServer::Http(McpServerHttp::new(name.clone(), url.clone()))
            }
        })
        .collect()
}

pub(super) fn build_session_setup(
    session_id: Option<&str>,
    cwd: impl AsRef<Path>,
    mcp_servers: &[AcpMcpServerConfig],
) -> Result<AcpSessionSetupRequest, Box<dyn Error>> {
    let cwd = validate_session_cwd(cwd.as_ref())?;
    let sdk_mcp_servers = convert_mcp_servers(mcp_servers);
    match session_id.map(str::trim) {
        Some(sid) if !sid.is_empty() => {
            let mut req = LoadSessionRequest::new(SessionId::new(sid), cwd);
            if !sdk_mcp_servers.is_empty() {
                req = req.mcp_servers(sdk_mcp_servers);
            }
            Ok(AcpSessionSetupRequest::Load(req))
        }
        _ => {
            let mut req = NewSessionRequest::new(cwd);
            if !sdk_mcp_servers.is_empty() {
                req = req.mcp_servers(sdk_mcp_servers);
            }
            Ok(AcpSessionSetupRequest::New(req))
        }
    }
}
