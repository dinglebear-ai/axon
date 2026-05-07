use agent_client_protocol::{
    ContentBlock, ContentChunk, PermissionOption, PermissionOptionId, PermissionOptionKind,
    RequestPermissionRequest, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionNotification, SessionUpdate,
    ToolCallContent, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
};
use axon::services::acp::{
    map_permission_request, map_permission_request_event, map_session_notification,
    map_session_notification_event, map_session_update_kind,
};
use axon::services::events::ServiceEvent;
use axon::services::types::{AcpBridgeEvent, AcpSessionUpdateKind};

#[test]
fn map_session_notification_agent_delta_extracts_text_and_kind() {
    let notification = SessionNotification::new(
        "session-123",
        SessionUpdate::AgentMessageChunk(ContentChunk::new("delta text".into())),
    );
    let mapped = map_session_notification(&notification);
    assert_eq!(mapped.session_id, "session-123");
    assert_eq!(mapped.kind, AcpSessionUpdateKind::AssistantDelta);
    assert_eq!(mapped.text_delta.as_deref(), Some("delta text"));
    assert!(mapped.tool_call_id.is_none());
}

#[test]
fn map_session_update_kind_tool_call_update_is_typed() {
    let update = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new("tool-1"),
        ToolCallUpdateFields::new().title("Read file"),
    ));
    assert_eq!(
        map_session_update_kind(&update),
        AcpSessionUpdateKind::ToolCallUpdated
    );
}

#[test]
fn map_permission_request_extracts_option_ids_and_tool_call() {
    let permission_request = RequestPermissionRequest::new(
        "session-77",
        ToolCallUpdate::new(
            ToolCallId::new("tool-77"),
            ToolCallUpdateFields::new().title("Write file"),
        ),
        vec![
            PermissionOption::new(
                PermissionOptionId::new("allow-once"),
                "Allow once",
                PermissionOptionKind::AllowOnce,
            ),
            PermissionOption::new(
                PermissionOptionId::new("reject-once"),
                "Reject once",
                PermissionOptionKind::RejectOnce,
            ),
        ],
    );

    let mapped = map_permission_request(&permission_request);
    assert_eq!(mapped.session_id, "session-77");
    assert_eq!(mapped.tool_call_id, "tool-77");
    assert_eq!(
        mapped.option_ids,
        vec!["allow-once".to_string(), "reject-once".to_string()]
    );
}

#[test]
fn map_session_notification_event_wraps_bridge_event() {
    let notification = SessionNotification::new(
        "session-wrap",
        SessionUpdate::AgentMessageChunk(ContentChunk::new("wrapped delta".into())),
    );

    let event = map_session_notification_event(&notification);
    match event {
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::SessionUpdate(update),
        } => {
            assert_eq!(update.session_id, "session-wrap");
            assert_eq!(update.kind, AcpSessionUpdateKind::AssistantDelta);
            assert_eq!(update.text_delta.as_deref(), Some("wrapped delta"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn map_session_notification_event_emits_config_options_update_for_config_changes() {
    let option = SessionConfigOption::new(
        "choice",
        "Model Selector",
        SessionConfigKind::Select(agent_client_protocol::SessionConfigSelect::new(
            "gpt-5.4",
            SessionConfigSelectOptions::Ungrouped(vec![
                SessionConfigSelectOption::new("gpt-5.4", "GPT 5.4"),
                SessionConfigSelectOption::new("gpt-5.3-codex", "GPT 5.3 Codex"),
            ]),
        )),
    )
    .category(SessionConfigOptionCategory::Model);
    let notification = SessionNotification::new(
        "session-config",
        SessionUpdate::ConfigOptionUpdate(agent_client_protocol::ConfigOptionUpdate::new(vec![
            option,
        ])),
    );

    let event = map_session_notification_event(&notification);
    match event {
        ServiceEvent::AcpBridge {
            event:
                AcpBridgeEvent::ConfigOptionsUpdate {
                    session_id,
                    config_options: options,
                },
        } => {
            assert_eq!(session_id, "session-config");
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].id, "choice");
            assert_eq!(options[0].category.as_deref(), Some("model"));
            assert_eq!(options[0].options.len(), 2);
            assert_eq!(options[0].options[0].value, "gpt-5.4");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn map_permission_request_event_wraps_bridge_event() {
    let permission_request = RequestPermissionRequest::new(
        "session-wrap-2",
        ToolCallUpdate::new(
            ToolCallId::new("tool-wrap"),
            ToolCallUpdateFields::new().title("Wrapped request"),
        ),
        vec![PermissionOption::new(
            PermissionOptionId::new("allow"),
            "Allow",
            PermissionOptionKind::AllowOnce,
        )],
    );

    let event = map_permission_request_event(&permission_request);
    match event {
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::PermissionRequest(req),
        } => {
            assert_eq!(req.session_id, "session-wrap-2");
            assert_eq!(req.tool_call_id, "tool-wrap");
            assert_eq!(req.option_ids, vec!["allow".to_string()]);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn map_session_notification_extracts_tool_content_text() {
    let update = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new("tool-content-1"),
        ToolCallUpdateFields::new()
            .title("Bash")
            .content(vec![ToolCallContent::from(ContentBlock::from(
                "hello world",
            ))]),
    ));
    let notification = SessionNotification::new("session-content", update);
    let mapped = map_session_notification(&notification);
    assert_eq!(mapped.tool_call_id.as_deref(), Some("tool-content-1"));
    assert_eq!(mapped.tool_content.as_deref(), Some("hello world"));
}

#[test]
fn map_session_notification_extracts_tool_raw_input() {
    let input_json = serde_json::json!({"command": "ls -la"});
    let update = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        ToolCallId::new("tool-input-1"),
        ToolCallUpdateFields::new()
            .title("Bash")
            .raw_input(input_json.clone()),
    ));
    let notification = SessionNotification::new("session-input", update);
    let mapped = map_session_notification(&notification);
    assert_eq!(mapped.tool_input.as_ref(), Some(&input_json));
}

#[test]
fn session_fallback_serializes_correctly() {
    let event = AcpBridgeEvent::SessionFallback {
        old_session_id: "old-123".to_string(),
        new_session_id: "new-456".to_string(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "session_fallback");
    assert_eq!(json["old_session_id"], "old-123");
    assert_eq!(json["new_session_id"], "new-456");
}

// ── Security regression: Unknown wire type disambiguation ───────────────────

#[test]
fn unknown_session_update_produces_unknown_wire_type_not_status() {
    use axon::services::types::AcpSessionUpdateEvent;

    let event = AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent {
        session_id: "session-unknown-test".to_string(),
        kind: AcpSessionUpdateKind::Unknown,
        text_delta: None,
        tool_call_id: None,
        tool_name: None,
        tool_status: None,
        tool_content: None,
        tool_input: None,
        tool_locations: None,
        kind_detail: None,
        message_id: None,
    });

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(
        json["type"], "unknown",
        "AcpBridgeEvent wire type for Unknown must be 'unknown', not 'status'; got: {}",
        json["type"]
    );
}

#[test]
fn plan_session_update_falls_through_as_status_wire_type() {
    use axon::services::types::AcpSessionUpdateEvent;

    // Plan, AvailableCommandsUpdate, CurrentModeUpdate, ConfigOptionUpdate are
    // normally intercepted by the AcpBridgeEvent custom Serialize. If they
    // somehow fall through to the SessionUpdate path, they produce "status".
    let event = AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent {
        session_id: "session-unknown-test".to_string(),
        kind: AcpSessionUpdateKind::Plan,
        text_delta: None,
        tool_call_id: None,
        tool_name: None,
        tool_status: None,
        tool_content: None,
        tool_input: None,
        tool_locations: None,
        kind_detail: None,
        message_id: None,
    });

    let json = serde_json::to_value(&event).unwrap();
    // Plan falls through Display to "status" — distinct from Unknown's "unknown".
    assert_eq!(
        json["type"], "status",
        "Plan fallthrough wire type must be 'status', got: {}",
        json["type"]
    );
}

#[test]
fn plan_update_event_uses_dedicated_wire_type() {
    use axon::services::types::AcpPlanUpdate;

    // When properly routed via AcpBridgeEvent::PlanUpdate (not SessionUpdate),
    // the wire type is "plan_update".
    let event = AcpBridgeEvent::PlanUpdate(AcpPlanUpdate {
        session_id: "sess-plan".to_string(),
        entries: vec![],
    });

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "plan_update");
    assert_eq!(json["session_id"], "sess-plan");
}

// ── New unstable variant tests ───────────────────────────────────────────────

#[test]
fn usage_update_maps_to_usage_update_kind() {
    use agent_client_protocol::UsageUpdate;
    let sdk_update = SessionUpdate::UsageUpdate(UsageUpdate::new(1000, 8000));
    let kind = map_session_update_kind(&sdk_update);
    assert_eq!(kind, AcpSessionUpdateKind::UsageUpdate);
}

#[test]
fn session_info_update_maps_to_session_info_update_bridge_event() {
    use agent_client_protocol::SessionInfoUpdate;
    let notification = SessionNotification::new(
        "test-session-id",
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()),
    );
    let event = map_session_notification_event(&notification);
    match event {
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::SessionInfoUpdate { session_id, .. },
        } => {
            assert_eq!(session_id, "test-session-id");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn usage_update_maps_through_notification_as_usage_update() {
    use agent_client_protocol::UsageUpdate;
    let notification = SessionNotification::new(
        "sess-usage",
        SessionUpdate::UsageUpdate(UsageUpdate::new(1000, 8000)),
    );
    let event = map_session_notification_event(&notification);
    match event {
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::UsageUpdate(usage),
        } => {
            assert_eq!(usage.session_id, "sess-usage");
            assert_eq!(usage.used, 1000);
            assert_eq!(usage.size, 8000);
            assert!(usage.cost_amount.is_none());
            assert!(usage.cost_currency.is_none());
        }
        other => panic!("expected UsageUpdate, got: {other:?}"),
    }
}
