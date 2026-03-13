//! WebSocket event types for the `axon serve` execution bridge.
//!
//! All variants of [`WsEventV2`] are serialized as JSON with a `"type"` tag
//! and consumed by `apps/web`. Fields not constructed in Rust may still be
//! active wire protocol members.
use crate::crates::services::types::AcpBridgeEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandContext {
    pub exec_id: String,
    pub mode: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobStatusPayload {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobProgressPayload {
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandDonePayload {
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandErrorPayload {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobCancelResponsePayload {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum WsEventV2 {
    #[serde(rename = "command.start")]
    CommandStart { ctx: CommandContext },
    #[serde(rename = "command.output.json")]
    CommandOutputJson { ctx: CommandContext, data: Value },
    #[serde(rename = "command.output.line")]
    CommandOutputLine { ctx: CommandContext, line: String },
    #[serde(rename = "command.done")]
    CommandDone {
        ctx: CommandContext,
        payload: CommandDonePayload,
    },
    #[serde(rename = "command.error")]
    CommandError {
        ctx: CommandContext,
        payload: CommandErrorPayload,
    },
    /// NOTE: Defined for future push-model implementation.
    /// As of 2026-03, async job progress is POLL-ONLY via the HTTP /api/jobs/:id endpoint.
    /// This variant is never emitted at runtime. Do not rely on it.
    #[serde(rename = "job.status")]
    JobStatus {
        ctx: CommandContext,
        payload: JobStatusPayload,
    },
    /// NOTE: Defined for future push-model implementation.
    /// As of 2026-03, async job progress is POLL-ONLY via the HTTP /api/jobs/:id endpoint.
    /// This variant is never emitted at runtime. Do not rely on it.
    #[serde(rename = "job.progress")]
    JobProgress {
        ctx: CommandContext,
        payload: JobProgressPayload,
    },
    #[serde(rename = "artifact.list")]
    ArtifactList {
        ctx: CommandContext,
        artifacts: Vec<ArtifactEntry>,
    },
    #[serde(rename = "artifact.content")]
    ArtifactContent {
        ctx: CommandContext,
        path: String,
        content: String,
    },
    #[serde(rename = "job.cancel.response")]
    JobCancelResponse {
        ctx: CommandContext,
        payload: JobCancelResponsePayload,
    },
}

pub(super) fn serialize_v2_event(event: WsEventV2) -> Option<String> {
    serde_json::to_string(&event)
        .map_err(|e| log::error!("failed to serialize WsEventV2: {e}"))
        .ok()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn acp_bridge_event_payload(event: &AcpBridgeEvent) -> Value {
    serde_json::to_value(event).unwrap_or_else(|e| {
        log::error!("failed to serialize ACP bridge event payload, sending error placeholder: {e}");
        serde_json::json!({ "error": format!("serialization failed: {e}") })
    })
}

/// Serialize an [`AcpBridgeEvent`] directly to a JSON string, skipping the
/// intermediate [`Value`] allocation used by [`acp_bridge_event_payload`].
///
/// This is the hot path for streaming tokens: one `serde_json::to_string` call
/// instead of two (`to_value` + `to_string`).
pub(super) fn acp_bridge_event_json(event: &AcpBridgeEvent) -> String {
    serde_json::to_string(event).unwrap_or_else(|e| {
        log::error!("failed to serialize ACP bridge event: {e}");
        // Use serde_json to properly escape the error message — a raw format!()
        // produces invalid JSON if the error contains quotes, backslashes, or newlines.
        serde_json::json!({"type": "error", "message": format!("serialization failed: {e}")})
            .to_string()
    })
}

/// Build a `command.output.json` WS envelope around a pre-serialized data
/// payload.  This avoids double serialization on the streaming hot path
/// (PERF-4): the `data_json` string is embedded verbatim into the envelope
/// rather than being parsed into `Value` and re-serialized.
///
/// # Wire type: `"command.output.json"`
///
/// This function intentionally uses `"command.output.json"` rather than the
/// basic `"output"` type from the legacy 5-type WS contract.  The legacy table
/// (`output`, `log`, `done`, `error`, `stats`) covers subprocess-backed modes;
/// direct-dispatch (ACP/pulse_chat) uses the richer `WsEventV2` protocol which
/// adds `"command.output.json"` for structured JSON payloads with execution
/// context.  Both names remain in use — `"output"` for raw subprocess lines,
/// `"command.output.json"` for typed service-layer results.
///
/// See `WsEventV2::CommandOutputJson` for the canonical definition.
///
/// The output matches the wire format of `WsEventV2::CommandOutputJson`:
/// ```json
/// {"type":"command.output.json","data":{"ctx":{...},"data":<data_json>}}
/// ```
pub(super) fn serialize_raw_output_event(ctx: &CommandContext, data_json: &str) -> Option<String> {
    let ctx_json = serde_json::to_string(ctx)
        .map_err(|e| log::error!("failed to serialize CommandContext: {e}"))
        .ok()?;
    // Build the envelope by string concatenation — `data_json` is already valid
    // JSON from `acp_bridge_event_json`, so no second serialization pass.
    Some(format!(
        r#"{{"type":"command.output.json","data":{{"ctx":{ctx_json},"data":{data_json}}}}}"#
    ))
}
