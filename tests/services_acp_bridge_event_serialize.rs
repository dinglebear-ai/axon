//! Wire-shape serialization tests for [`AcpBridgeEvent`] and [`AcpSessionUpdateKind`].
//!
//! Moved from `crates/services/types/acp.rs` to reduce that module's line count
//! below the 500-line monolith policy limit.

use axon::crates::services::types::{
    AcpBridgeEvent, AcpConfigOption, AcpConfigSelectValue, AcpPermissionRequestEvent,
    AcpSessionUpdateEvent, AcpSessionUpdateKind, AcpTurnResultEvent, AcpUsageUpdate,
};
use serde_json::Value;

// ── AcpBridgeEvent::SessionUpdate (assistant_delta) ───────────────────────

#[test]
fn acpbridgeevent_assistant_delta_wire_shape() {
    let event = AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent {
        session_id: "s1".to_string(),
        kind: AcpSessionUpdateKind::AssistantDelta,
        text_delta: Some("hello".to_string()),
        tool_call_id: None,
        tool_name: None,
        tool_status: None,
        tool_content: None,
        tool_input: None,
        tool_locations: None,
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "assistant_delta");
    assert_eq!(v["delta"], "hello");
    // assistant_delta must NOT use the "content" key for its text
    assert!(
        v.get("content").is_none(),
        "assistant_delta must not have a 'content' key"
    );
    assert_eq!(v["session_id"], "s1");
}

// ── AcpBridgeEvent::SessionUpdate (thinking_content) ─────────────────────

#[test]
fn acpbridgeevent_thinking_content_wire_shape() {
    let event = AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent {
        session_id: "s2".to_string(),
        kind: AcpSessionUpdateKind::ThinkingDelta,
        text_delta: Some("thought".to_string()),
        tool_call_id: None,
        tool_name: None,
        tool_status: None,
        tool_content: None,
        tool_input: None,
        tool_locations: None,
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "thinking_content");
    // thinking_content uses "content", NOT "delta"
    assert_eq!(v["content"], "thought");
    assert!(
        v.get("delta").is_none(),
        "thinking_content must not have a 'delta' key"
    );
}

// ── AcpBridgeEvent::UsageUpdate ──────────────────────────────────────────

#[test]
fn acpbridgeevent_usage_update_wire_shape() {
    let event = AcpBridgeEvent::UsageUpdate(AcpUsageUpdate {
        session_id: "s-usage".to_string(),
        used: 1000,
        size: 8000,
        cost_amount: Some("0.05".to_string()),
        cost_currency: Some("USD".to_string()),
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "usage_update");
    assert_eq!(v["session_id"], "s-usage");
    assert_eq!(v["usage"]["total_tokens"], 1000);
    assert_eq!(v["size"], 8000);
    assert_eq!(v["costAmount"], "0.05");
    assert_eq!(v["costCurrency"], "USD");
    // "used" must NOT appear at top level — web client expects nested "usage" object
    assert!(v.get("used").is_none(), "used must not appear at top level");
}

#[test]
fn acpbridgeevent_usage_update_without_cost() {
    let event = AcpBridgeEvent::UsageUpdate(AcpUsageUpdate {
        session_id: "s2".to_string(),
        used: 500,
        size: 4000,
        cost_amount: None,
        cost_currency: None,
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "usage_update");
    assert_eq!(v["usage"]["total_tokens"], 500);
    assert_eq!(v["size"], 4000);
    assert!(v.get("costAmount").is_none());
    assert!(v.get("costCurrency").is_none());
    assert!(v.get("used").is_none(), "used must not appear at top level");
}

// ── AcpBridgeEvent::PermissionRequest ─────────────────────────────────────

#[test]
fn acpbridgeevent_permission_request_wire_shape() {
    let event = AcpBridgeEvent::PermissionRequest(AcpPermissionRequestEvent {
        session_id: "s3".to_string(),
        tool_call_id: "tc1".to_string(),
        option_ids: vec!["allow".to_string(), "deny".to_string()],
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "permission_request");
    assert!(v["options"].is_array(), "'options' must be an array");
    assert_eq!(v["options"].as_array().unwrap().len(), 2);
}

// ── AcpBridgeEvent::TurnResult ────────────────────────────────────────────

#[test]
fn acpbridgeevent_turn_result_wire_shape() {
    let event = AcpBridgeEvent::TurnResult(AcpTurnResultEvent {
        session_id: "s4".to_string(),
        stop_reason: "end_turn".to_string(),
        result: "done".to_string(),
    });
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "result");
    assert!(
        v.get("stop_reason").is_some(),
        "'stop_reason' must be present"
    );
    assert_eq!(v["stop_reason"], "end_turn");
}

// ── AcpBridgeEvent::ConfigOptionsUpdate ───────────────────────────────────

#[test]
fn acpbridgeevent_config_options_update_wire_shape() {
    let opt = AcpConfigOption {
        id: "model".to_string(),
        name: "Model".to_string(),
        description: None,
        category: None,
        current_value: "claude-3-5-sonnet".to_string(),
        options: vec![AcpConfigSelectValue {
            value: "claude-3-5-sonnet".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            description: None,
        }],
    };
    let event = AcpBridgeEvent::ConfigOptionsUpdate {
        session_id: "s5".to_string(),
        config_options: vec![opt],
    };
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "config_options_update");
    assert_eq!(v["session_id"], "s5");
    // The key must be camelCase "configOptions" per the custom Serialize impl
    assert!(
        v.get("configOptions").is_some(),
        "key must be 'configOptions' (camelCase)"
    );
    assert!(v["configOptions"].is_array());
}

// ── AcpBridgeEvent::SessionFallback ───────────────────────────────────────

#[test]
fn acpbridgeevent_session_fallback_wire_shape() {
    let event = AcpBridgeEvent::SessionFallback {
        old_session_id: "old-123".to_string(),
        new_session_id: "new-456".to_string(),
    };
    let v: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(v["type"], "session_fallback");
    assert_eq!(v["old_session_id"], "old-123");
    assert_eq!(v["new_session_id"], "new-456");
}

// ── AcpBridgeEvent::SessionInfoUpdate ─────────────────────────────────────

#[test]
fn session_info_update_serializes_correctly() {
    let event = AcpBridgeEvent::SessionInfoUpdate {
        session_id: "abc-123".to_string(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "session_info_update");
    assert_eq!(json["session_id"], "abc-123");
}

// ── AcpSessionUpdateKind Display ─────────────────────────────────────────

#[test]
fn acpsessionupdatekind_display_status_variants() {
    // These four variants display as "status" (they are intercepted by the
    // custom AcpBridgeEvent Serialize impl and never reach the wire via the
    // SessionUpdate path in practice).
    let status_variants = [
        AcpSessionUpdateKind::Plan,
        AcpSessionUpdateKind::AvailableCommandsUpdate,
        AcpSessionUpdateKind::CurrentModeUpdate,
        AcpSessionUpdateKind::ConfigOptionUpdate,
    ];
    for kind in &status_variants {
        assert_eq!(
            kind.to_string(),
            "status",
            "{kind:?} Display must produce \"status\""
        );
    }
    // Unknown now displays as "unknown" (H-3/A-4 fix: disambiguated from "status")
    assert_eq!(
        AcpSessionUpdateKind::Unknown.to_string(),
        "unknown",
        "Unknown Display must produce \"unknown\""
    );
    // Sanity-check the non-status variants
    assert_eq!(AcpSessionUpdateKind::UserDelta.to_string(), "user_delta");
    assert_eq!(
        AcpSessionUpdateKind::AssistantDelta.to_string(),
        "assistant_delta"
    );
    assert_eq!(
        AcpSessionUpdateKind::ThinkingDelta.to_string(),
        "thinking_content"
    );
    assert_eq!(
        AcpSessionUpdateKind::ToolCallStarted.to_string(),
        "tool_use"
    );
    assert_eq!(
        AcpSessionUpdateKind::ToolCallUpdated.to_string(),
        "tool_use_update"
    );
    assert_eq!(
        AcpSessionUpdateKind::UsageUpdate.to_string(),
        "usage_update"
    );
}
