use crate::events::RecordedEvent;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Instant;

fn approval_decision_supported(method: &str) -> bool {
    method == "applyPatchApproval"
        || method == "execCommandApproval"
        || method.ends_with("requestApproval")
}

pub(super) fn server_request_result(
    method: &str,
    approved: bool,
    typed_response: Option<Value>,
) -> Result<Value, String> {
    if approval_decision_supported(method) {
        return Ok(json!({"decision": if approved { "accept" } else { "decline" }}));
    }
    if method == "item/tool/requestUserInput" {
        if !approved {
            return Ok(json!({"answers": {}}));
        }
        let response = typed_response.ok_or("requestUserInput approval requires typed answers")?;
        if !response.get("answers").is_some_and(Value::is_object) {
            return Err("requestUserInput response requires an answers object".to_string());
        }
        return Ok(response);
    }
    if method == "mcpServer/elicitation/request" {
        if !approved {
            return Ok(json!({"action": "decline"}));
        }
        let response = typed_response.ok_or("MCP elicitation approval requires typed content")?;
        if response.get("action").and_then(Value::as_str) != Some("accept")
            || !response.get("content").is_some_and(Value::is_object)
        {
            return Err(
                "MCP elicitation response requires action=accept and object content".to_string(),
            );
        }
        return Ok(response);
    }
    if approved {
        return Err(format!(
            "server request {method} cannot be generically approved"
        ));
    }
    Ok(json!({"decision":"decline"}))
}

#[derive(Clone)]
pub(super) struct PendingServerRequest {
    pub(super) method: String,
    pub(super) expires_at: Instant,
    pub(super) claimed: bool,
    pub(super) event: RecordedEvent,
}

pub(super) fn claim_server_request(
    registry: &mut HashMap<u64, PendingServerRequest>,
    request_id: u64,
) -> Result<(), String> {
    let pending_request = registry
        .get_mut(&request_id)
        .ok_or("server request is unknown or already answered")?;
    if pending_request.expires_at <= Instant::now() {
        registry.remove(&request_id);
        return Err("server request approval expired".to_string());
    }
    if pending_request.claimed {
        return Err("server request response is already in progress".to_string());
    }
    pending_request.claimed = true;
    Ok(())
}

pub(super) fn finish_server_request(
    registry: &mut HashMap<u64, PendingServerRequest>,
    request_id: u64,
    write_succeeded: bool,
) {
    if write_succeeded {
        registry.remove(&request_id);
    } else if let Some(pending_request) = registry.get_mut(&request_id) {
        pending_request.claimed = false;
    }
}
