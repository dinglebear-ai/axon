//! ACP SDK type mapping: converts `agent_client_protocol` types into our
//! service-layer representation (`AcpBridgeEvent`, `AcpSessionUpdateEvent`, etc.).

use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::{
    AcpAvailableCommand, AcpBridgeEvent, AcpCommandsUpdate, AcpConfigOption, AcpConfigSelectValue,
    AcpModeUpdate, AcpPermissionRequestEvent, AcpPlanEntry, AcpPlanUpdate, AcpSessionUpdateEvent,
    AcpSessionUpdateKind, AcpUsageUpdate,
};
use agent_client_protocol::{
    ContentBlock, SessionConfigKind, SessionConfigOption as SdkConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOptions, SessionNotification, SessionUpdate,
    ToolCallContent,
};

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
    let kind_detail = extract_tool_kind_detail(&notification.update);
    let message_id = extract_message_id(&notification.update);
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
        kind_detail,
        message_id,
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
            if let SessionConfigKind::Boolean(bool_config) = &opt.kind {
                let category = opt.category.as_ref().map(|c| match c {
                    SessionConfigOptionCategory::Mode => "mode".to_string(),
                    SessionConfigOptionCategory::Model => "model".to_string(),
                    SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
                    SessionConfigOptionCategory::Other(s) => s.clone(),
                    _ => "other".to_string(),
                });
                return Some(AcpConfigOption {
                    id: opt.id.0.to_string(),
                    name: opt.name.clone(),
                    description: opt.description.clone(),
                    category,
                    current_value: bool_config.current_value.to_string(),
                    options: vec![
                        AcpConfigSelectValue {
                            value: "true".to_string(),
                            name: "Enabled".to_string(),
                            description: None,
                        },
                        AcpConfigSelectValue {
                            value: "false".to_string(),
                            name: "Disabled".to_string(),
                            description: None,
                        },
                    ],
                });
            }
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
        SessionUpdate::SessionInfoUpdate(info) => ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::SessionInfoUpdate {
                session_id: sid,
                title: info.title.value().cloned(),
                updated_at: info.updated_at.value().cloned(),
            },
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

fn extract_diff_text(diff: &agent_client_protocol::Diff) -> Option<String> {
    let old = diff.old_text.as_deref().unwrap_or("");
    let new_text = &diff.new_text;
    let path = diff.path.display().to_string();
    Some(format!("--- {path}\n{old}\n+++ {path}\n{new_text}"))
}

fn extract_tool_content(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(tc) => tc.content.iter().find_map(|c| match c {
            ToolCallContent::Content(content) => extract_content_text(&content.content),
            ToolCallContent::Diff(diff) => extract_diff_text(diff),
            _ => None,
        }),
        SessionUpdate::ToolCallUpdate(tcu) => tcu.fields.content.as_ref().and_then(|contents| {
            contents.iter().find_map(|c| match c {
                ToolCallContent::Content(content) => extract_content_text(&content.content),
                ToolCallContent::Diff(diff) => extract_diff_text(diff),
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

fn tool_kind_to_str(kind: &agent_client_protocol::ToolKind) -> Option<String> {
    let s = format!("{kind:?}").to_lowercase();
    if s == "other" { None } else { Some(s) }
}

fn extract_tool_kind_detail(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(tc) => tool_kind_to_str(&tc.kind),
        SessionUpdate::ToolCallUpdate(tcu) => tcu.fields.kind.as_ref().and_then(tool_kind_to_str),
        _ => None,
    }
}

fn extract_message_id(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => chunk.message_id.clone(),
        _ => None,
    }
}

// ── Validation (extracted to mapping/validation.rs) ─────────────────────────

mod validation;
pub use validation::{
    validate_adapter_command, validate_probe_request, validate_prompt_turn_request,
    validate_session_cwd,
};

// ── MCP server capability filters (extracted to mapping/mcp_filters.rs) ──────

mod mcp_filters;
#[cfg(test)]
pub(super) use mcp_filters::filter_compatible_mcp_servers;
pub(super) use mcp_filters::filter_sdk_mcp_servers;

// ── Session setup helpers (extracted to mapping/session_setup.rs) ─────────────

mod session_setup;
pub(super) use session_setup::{build_session_setup, convert_mcp_servers};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::AcpMcpServerConfig;
    use agent_client_protocol::{McpServer, McpServerHttp, McpServerSse, McpServerStdio};

    #[test]
    fn test_message_id_forwarded() {
        // RED: AcpSessionUpdateEvent has no message_id field yet.
        // This test fails to compile until Task 1.26 adds the field.
        use agent_client_protocol::{
            ContentBlock, ContentChunk, SessionId, SessionNotification, SessionUpdate,
        };
        let chunk = ContentChunk::new(ContentBlock::Text(agent_client_protocol::TextContent::new(
            "hello",
        )))
        .message_id("msg-1".to_string());
        let notification = SessionNotification::new(
            SessionId::new("s1"),
            SessionUpdate::AgentMessageChunk(chunk),
        );
        let event = map_session_notification(&notification);
        // message_id field will not exist until Task 1.26 — compile error here
        assert_eq!(event.message_id, Some("msg-1".to_string()));
    }

    #[test]
    fn filter_keeps_stdio_always() {
        let servers = vec![AcpMcpServerConfig::Stdio {
            name: "s".into(),
            command: "/bin/srv".into(),
            args: vec![],
            env: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_drops_http_when_not_supported() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(),
            url: "http://localhost/mcp".into(),
            headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_http_when_supported() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(),
            url: "http://localhost/mcp".into(),
            headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, true, false);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_drops_sse_when_not_supported() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(),
            url: "http://localhost/sse".into(),
            headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_sse_when_supported() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(),
            url: "http://localhost/sse".into(),
            headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, true);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_diff_content_extraction() {
        use agent_client_protocol::{Diff, SessionUpdate, ToolCall, ToolCallContent, ToolCallId};
        let diff = Diff::new("/path/to/file.rs", "new content").old_text("old content".to_string());
        let tool_call = ToolCall::new(ToolCallId::new("call_1"), "str_replace_based_edit_tool")
            .content(vec![ToolCallContent::Diff(diff)]);
        let update = SessionUpdate::ToolCall(tool_call);
        let result = extract_tool_content(&update);
        assert!(result.is_some(), "expected Some but got None");
        let text = result.unwrap();
        assert!(
            text.contains("old content"),
            "expected old_text in result: {text}"
        );
        assert!(
            text.contains("new content"),
            "expected new_text in result: {text}"
        );
    }

    #[test]
    fn convert_http_with_headers() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(),
            url: "http://localhost/mcp".into(),
            headers: vec![("Authorization".to_string(), "Bearer tok".to_string())],
        }];
        let sdk = convert_mcp_servers(&servers);
        assert_eq!(sdk.len(), 1);
        match &sdk[0] {
            McpServer::Http(h) => {
                assert_eq!(h.headers.len(), 1);
                assert_eq!(h.headers[0].name, "Authorization");
                assert_eq!(h.headers[0].value, "Bearer tok");
            }
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn convert_sse_maps_correctly() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(),
            url: "http://localhost/sse".into(),
            headers: vec![],
        }];
        let sdk = convert_mcp_servers(&servers);
        assert_eq!(sdk.len(), 1);
        assert!(matches!(sdk[0], McpServer::Sse(_)));
    }

    #[test]
    fn filter_sdk_drops_http_when_not_supported() {
        let servers = vec![McpServer::Http(McpServerHttp::new(
            "h",
            "http://localhost/mcp",
        ))];
        let filtered = filter_sdk_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_sdk_keeps_stdio_always() {
        let servers = vec![McpServer::Stdio(McpServerStdio::new("s", "/bin/srv"))];
        let filtered = filter_sdk_mcp_servers(&servers, false, false);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_sdk_drops_sse_when_not_supported() {
        let servers = vec![McpServer::Sse(McpServerSse::new(
            "s",
            "http://localhost/sse",
        ))];
        let filtered = filter_sdk_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_sdk_keeps_sse_when_supported() {
        let servers = vec![McpServer::Sse(McpServerSse::new(
            "s",
            "http://localhost/sse",
        ))];
        let filtered = filter_sdk_mcp_servers(&servers, false, true);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_boolean_config_option_mapping() {
        use agent_client_protocol::{SessionConfigBoolean, SessionConfigKind, SessionConfigOption};
        let opt = SessionConfigOption::new(
            "auto_compact",
            "Auto Compact",
            SessionConfigKind::Boolean(SessionConfigBoolean::new(true)),
        );
        let result = map_config_options(&[opt]);
        assert!(
            !result.is_empty(),
            "expected Boolean config to produce options"
        );
    }

    #[test]
    fn test_extract_content_diff_none_old_text() {
        use agent_client_protocol::{Diff, SessionUpdate, ToolCall, ToolCallContent, ToolCallId};
        // Simulate new file creation: old_text is None, only new_text present.
        let diff = Diff::new("/path/to/new_file.rs", "fn main() {}");
        let tool_call = ToolCall::new(
            ToolCallId::new("call_new_file"),
            "str_replace_based_edit_tool",
        )
        .content(vec![ToolCallContent::Diff(diff)]);
        let update = SessionUpdate::ToolCall(tool_call);
        let result = extract_tool_content(&update);
        assert!(
            result.is_some(),
            "expected Some for Diff with None old_text"
        );
        let text = result.unwrap();
        assert!(
            text.contains("fn main() {}"),
            "expected new_text in result: {text}"
        );
        // old_text is None → rendered as empty string between the markers
        assert!(
            !text.contains("old content"),
            "result should not contain stale old content: {text}"
        );
    }

    #[test]
    fn test_map_config_boolean_two_options() {
        use agent_client_protocol::{SessionConfigBoolean, SessionConfigKind, SessionConfigOption};
        let opt = SessionConfigOption::new(
            "verbose_mode",
            "Verbose Mode",
            SessionConfigKind::Boolean(SessionConfigBoolean::new(false)),
        );
        let result = map_config_options(&[opt]);
        assert_eq!(result.len(), 1, "expected exactly one config option");
        let config = &result[0];
        assert_eq!(config.id, "verbose_mode");
        assert_eq!(
            config.options.len(),
            2,
            "Boolean must produce exactly two options"
        );
        let values: Vec<&str> = config.options.iter().map(|o| o.value.as_str()).collect();
        assert!(values.contains(&"true"), "expected 'true' option");
        assert!(values.contains(&"false"), "expected 'false' option");
    }
}
