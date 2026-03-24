//! ACP (Agent Communication Protocol) types — session setup, bridge events,
//! update kinds, permission requests, and config options.

// ── Session setup / adapter ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpAdapterCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// When true (default), advertise filesystem capability during initialize.
    pub enable_fs: bool,
    /// When true (default), advertise terminal capability during initialize.
    pub enable_terminal: bool,
    /// Timeout for frontend to respond to permission requests (seconds).
    pub permission_timeout_secs: Option<u64>,
    /// Overall process execution timeout (seconds).
    pub adapter_timeout_secs: Option<u64>,
}

impl AcpAdapterCommand {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: None,
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        }
    }

    pub fn new_full(
        program: impl Into<String>,
        args: Vec<String>,
        cwd: Option<String>,
        enable_fs: bool,
        enable_terminal: bool,
        permission_timeout_secs: Option<u64>,
        adapter_timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            cwd,
            enable_fs,
            enable_terminal,
            permission_timeout_secs,
            adapter_timeout_secs,
        }
    }
}

impl Default for AcpAdapterCommand {
    fn default() -> Self {
        Self {
            program: String::new(),
            args: Vec::new(),
            cwd: None,
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpPromptTurnRequest {
    pub session_id: Option<String>,
    pub prompt: Vec<String>,
    /// Model config option value to set after session setup (if agent supports it).
    pub model: Option<String>,
    /// Session mode / approval config value to apply on the active session.
    pub session_mode: Option<String>,
    /// MCP tool names (command IDs) blocked for this turn/session runtime.
    pub blocked_mcp_tools: Vec<String>,
    /// MCP servers to pass through to the ACP agent session.
    pub mcp_servers: Vec<AcpMcpServerConfig>,
}

/// MCP server configuration passed through to ACP NewSessionRequest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum AcpMcpServerConfig {
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: Vec<(String, String)>,
    },
    Http {
        name: String,
        url: String,
        /// HTTP headers to send with every request (name, value) pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
    Sse {
        name: String,
        url: String,
        /// HTTP headers to send with every SSE request (name, value) pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
}

impl AcpMcpServerConfig {
    /// Returns the server name regardless of transport variant.
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio { name, .. } | Self::Http { name, .. } | Self::Sse { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpSessionProbeRequest {
    pub session_id: Option<String>,
    /// Optional model config option value to apply during probe.
    pub model: Option<String>,
}

// ── Config options ───────────────────────────────────────────────────────────

/// A single selectable value within an ACP config option.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpConfigSelectValue {
    pub value: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// An ACP session config option (model selector, mode selector, etc.).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpConfigOption {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub current_value: String,
    pub options: Vec<AcpConfigSelectValue>,
}

// ── Session update kind ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AcpSessionUpdateKind {
    #[serde(rename = "user_delta")]
    UserDelta,
    #[serde(rename = "assistant_delta")]
    AssistantDelta,
    #[serde(rename = "thinking_content")]
    ThinkingDelta,
    #[serde(rename = "tool_use")]
    ToolCallStarted,
    #[serde(rename = "tool_use_update")]
    ToolCallUpdated,
    /// Intercepted by `AcpBridgeEvent::PlanUpdate` custom serialization — never
    /// reaches the `AcpSessionUpdateKind` serde path directly.
    #[serde(rename = "plan")]
    Plan,
    /// Intercepted by `AcpBridgeEvent::CommandsUpdate` custom serialization.
    #[serde(rename = "available_commands_update")]
    AvailableCommandsUpdate,
    /// Intercepted by `AcpBridgeEvent::ModeUpdate` custom serialization.
    #[serde(rename = "current_mode_update")]
    CurrentModeUpdate,
    /// Intercepted by `AcpBridgeEvent::ConfigOptionsUpdate` custom serialization.
    #[serde(rename = "config_option_update")]
    ConfigOptionUpdate,
    /// Context window usage stats from the ACP agent.
    #[serde(rename = "usage_update")]
    UsageUpdate,
    /// Catch-all for unrecognized ACP protocol events. Serialized as `"unknown"`
    /// so it is distinguishable on the wire (previously collided with `"status"`).
    #[serde(rename = "unknown")]
    Unknown,
}

impl std::fmt::Display for AcpSessionUpdateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserDelta => write!(f, "user_delta"),
            Self::AssistantDelta => write!(f, "assistant_delta"),
            Self::ThinkingDelta => write!(f, "thinking_content"),
            Self::ToolCallStarted => write!(f, "tool_use"),
            Self::ToolCallUpdated => write!(f, "tool_use_update"),
            // These four are intercepted by AcpBridgeEvent's custom Serialize
            // impl and routed to dedicated wire types (plan_update, commands_update,
            // mode_update, config_options_update). The Display value here is only
            // used if the event falls through to the SessionUpdate serialization
            // path — which means the AcpBridgeEvent dispatch didn't match it.
            // In that case, "status" is the correct legacy wire type.
            Self::Plan => write!(f, "status"),
            Self::AvailableCommandsUpdate => write!(f, "status"),
            Self::CurrentModeUpdate => write!(f, "status"),
            Self::ConfigOptionUpdate => write!(f, "status"),
            Self::UsageUpdate => write!(f, "usage_update"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// ── Session update event ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AcpSessionUpdateEvent {
    pub session_id: String,
    pub kind: AcpSessionUpdateKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    /// Text content produced by the tool call (extracted from ToolCallContent::Content).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_content: Option<String>,
    /// Raw JSON input to the tool call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    /// File paths or URIs associated with the tool call (e.g. read/write targets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_locations: Option<Vec<String>>,
    /// Tool kind forwarded from the ACP ToolCall (e.g. "read", "edit", "execute").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_detail: Option<String>,
    /// Message identifier forwarded from `ContentChunk.message_id`.
    /// Groups multiple chunks belonging to the same logical message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

// ── Permission request ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AcpPermissionRequestEvent {
    pub session_id: String,
    pub tool_call_id: String,
    /// Serialized as `"options"` on the wire to match existing frontend expectations.
    #[serde(rename = "options")]
    pub option_ids: Vec<String>,
}

// ── Turn result ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AcpTurnResultEvent {
    pub session_id: String,
    pub stop_reason: String,
    pub result: String,
}

// ── Plan / mode / commands updates ───────────────────────────────────────────

/// ACP plan forwarded from the agent's Plan session update.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpPlanEntry {
    pub content: String,
    pub priority: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpPlanUpdate {
    pub session_id: String,
    pub entries: Vec<AcpPlanEntry>,
}

/// Current mode changed notification from the ACP agent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpModeUpdate {
    pub session_id: String,
    pub current_mode_id: String,
}

/// Available slash-commands changed notification from the ACP agent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAvailableCommand {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpCommandsUpdate {
    pub session_id: String,
    pub commands: Vec<AcpAvailableCommand>,
}

/// Context window usage stats forwarded from the ACP agent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpUsageUpdate {
    pub session_id: String,
    /// Tokens currently in context.
    pub used: u64,
    /// Total context window size in tokens.
    pub size: u64,
    /// Cumulative session cost amount (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_amount: Option<String>,
    /// ISO 4217 currency code for cost (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_currency: Option<String>,
}

// ── Elicitation request (FR-031) ─────────────────────────────────────────────

/// Elicitation request forwarded from the ACP agent to the frontend.
///
/// The ACP agent may request additional information from the user via
/// `unstable_elicitation`. This event carries the prompt and optional
/// schema so the frontend can render a form or free-text input.
///
/// TODO(FR-031): Populate from the real SDK `ElicitRequest` type once
/// `agent_client_protocol` exposes `unstable_elicitation` (SDK v0.10.2 does not).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpElicitRequest {
    pub session_id: String,
    /// Human-readable prompt shown above the elicitation form.
    pub message: String,
    /// Optional JSON schema describing the expected response shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

// ── Bridge event (top-level enum) ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpBridgeEvent {
    SessionUpdate(AcpSessionUpdateEvent),
    PermissionRequest(AcpPermissionRequestEvent),
    TurnResult(AcpTurnResultEvent),
    /// Config options changed. Carries the session that received the update
    /// so the frontend can correlate it with the correct agent panel.
    ConfigOptionsUpdate {
        session_id: String,
        config_options: Vec<AcpConfigOption>,
    },
    PlanUpdate(AcpPlanUpdate),
    ModeUpdate(AcpModeUpdate),
    CommandsUpdate(AcpCommandsUpdate),
    SessionFallback {
        old_session_id: String,
        new_session_id: String,
    },
    /// Context window usage update from the ACP agent.
    UsageUpdate(AcpUsageUpdate),
    /// Session metadata was updated (title, updated_at). The session_id is the
    /// ID of the session that received the update.
    SessionInfoUpdate {
        session_id: String,
        title: Option<String>,
        updated_at: Option<String>,
    },
    /// Elicitation request from the ACP agent (FR-031).
    ///
    /// Forwarded to the frontend so the user can provide additional input.
    /// TODO(FR-031): Wire this to the real SDK callback once `unstable_elicitation`
    /// is available in `agent_client_protocol`.
    ElicitRequest(AcpElicitRequest),
}

// ── Per-variant serialization helpers ────────────────────────────────────────

fn serialize_session_update<S: serde::Serializer>(
    update: &AcpSessionUpdateEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    if update.kind == AcpSessionUpdateKind::Unknown {
        tracing::warn!(
            context = "acp_types",
            update = ?update,
            "received unknown ACP session update kind, forwarding as 'unknown'"
        );
    }
    let event_type = update.kind.to_string();
    let text_key = if event_type == "thinking_content" {
        "content"
    } else {
        "delta"
    };
    let text_val = update.text_delta.clone().unwrap_or_default();
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", &event_type)?;
    map.serialize_entry("session_id", &update.session_id)?;
    map.serialize_entry("tool_call_id", &update.tool_call_id)?;
    map.serialize_entry(text_key, &text_val)?;
    if let Some(ref name) = update.tool_name {
        map.serialize_entry("tool_name", name)?;
    }
    if let Some(ref status) = update.tool_status {
        map.serialize_entry("tool_status", status)?;
    }
    if let Some(ref content) = update.tool_content {
        map.serialize_entry("tool_content", content)?;
    }
    if let Some(ref input) = update.tool_input {
        map.serialize_entry("tool_input", input)?;
    }
    if let Some(ref locations) = update.tool_locations {
        map.serialize_entry("tool_locations", locations)?;
    }
    if let Some(ref kind) = update.kind_detail {
        map.serialize_entry("kind", kind)?;
    }
    if let Some(ref mid) = update.message_id {
        map.serialize_entry("message_id", mid)?;
    }
    map.end()
}

fn serialize_permission_request<S: serde::Serializer>(
    req: &AcpPermissionRequestEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "permission_request")?;
    map.serialize_entry("session_id", &req.session_id)?;
    map.serialize_entry("tool_call_id", &req.tool_call_id)?;
    map.serialize_entry("options", &req.option_ids)?;
    map.end()
}

fn serialize_turn_result<S: serde::Serializer>(
    result: &AcpTurnResultEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "result")?;
    map.serialize_entry("session_id", &result.session_id)?;
    map.serialize_entry("stop_reason", &result.stop_reason)?;
    map.serialize_entry("result", &result.result)?;
    map.end()
}

fn serialize_config_options_update<S: serde::Serializer>(
    session_id: &str,
    options: &[AcpConfigOption],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "config_options_update")?;
    map.serialize_entry("session_id", session_id)?;
    map.serialize_entry("configOptions", options)?;
    map.end()
}

fn serialize_plan_update<S: serde::Serializer>(
    plan: &AcpPlanUpdate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "plan_update")?;
    map.serialize_entry("session_id", &plan.session_id)?;
    map.serialize_entry("entries", &plan.entries)?;
    map.end()
}

fn serialize_mode_update<S: serde::Serializer>(
    mode: &AcpModeUpdate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "mode_update")?;
    map.serialize_entry("session_id", &mode.session_id)?;
    map.serialize_entry("currentModeId", &mode.current_mode_id)?;
    map.end()
}

fn serialize_commands_update<S: serde::Serializer>(
    cmds: &AcpCommandsUpdate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "commands_update")?;
    map.serialize_entry("session_id", &cmds.session_id)?;
    map.serialize_entry("commands", &cmds.commands)?;
    map.end()
}

fn serialize_usage_update<S: serde::Serializer>(
    usage: &AcpUsageUpdate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;

    // Build the nested "usage" object that the web client Zod schema expects:
    //   { total_tokens?: int, input_tokens?: int, output_tokens?: int }
    // We only have `used` (total tokens in context), so map it to `total_tokens`.
    // Include all three fields with explicit 0 for input/output — the web UI
    // treats the object as complete and crashes if expected fields are absent.
    let mut usage_obj = serde_json::Map::new();
    usage_obj.insert(
        "total_tokens".to_string(),
        serde_json::Value::Number(usage.used.into()),
    );
    usage_obj.insert(
        "input_tokens".to_string(),
        serde_json::Value::Number(0.into()),
    );
    usage_obj.insert(
        "output_tokens".to_string(),
        serde_json::Value::Number(0.into()),
    );

    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "usage_update")?;
    map.serialize_entry("session_id", &usage.session_id)?;
    map.serialize_entry("usage", &usage_obj)?;
    map.serialize_entry("size", &usage.size)?;
    if let Some(ref amount) = usage.cost_amount {
        map.serialize_entry("costAmount", amount)?;
    }
    if let Some(ref currency) = usage.cost_currency {
        map.serialize_entry("costCurrency", currency)?;
    }
    map.end()
}

fn serialize_session_info_update<S: serde::Serializer>(
    session_id: &str,
    title: Option<&str>,
    updated_at: Option<&str>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "session_info_update")?;
    map.serialize_entry("session_id", session_id)?;
    if let Some(t) = title {
        map.serialize_entry("title", t)?;
    }
    if let Some(u) = updated_at {
        map.serialize_entry("updated_at", u)?;
    }
    map.end()
}

fn serialize_elicit_request<S: serde::Serializer>(
    req: &AcpElicitRequest,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "elicit_request")?;
    map.serialize_entry("session_id", &req.session_id)?;
    map.serialize_entry("message", &req.message)?;
    if let Some(ref schema) = req.schema {
        map.serialize_entry("schema", schema)?;
    }
    map.end()
}

fn serialize_session_fallback<S: serde::Serializer>(
    old_session_id: &str,
    new_session_id: &str,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    map.serialize_entry("type", "session_fallback")?;
    map.serialize_entry("old_session_id", old_session_id)?;
    map.serialize_entry("new_session_id", new_session_id)?;
    map.end()
}

impl serde::Serialize for AcpBridgeEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::SessionUpdate(update) => serialize_session_update(update, serializer),
            Self::PermissionRequest(req) => serialize_permission_request(req, serializer),
            Self::TurnResult(result) => serialize_turn_result(result, serializer),
            Self::ConfigOptionsUpdate {
                session_id,
                config_options,
            } => serialize_config_options_update(session_id, config_options, serializer),
            Self::PlanUpdate(plan) => serialize_plan_update(plan, serializer),
            Self::ModeUpdate(mode) => serialize_mode_update(mode, serializer),
            Self::CommandsUpdate(cmds) => serialize_commands_update(cmds, serializer),
            Self::UsageUpdate(usage) => serialize_usage_update(usage, serializer),
            Self::SessionFallback {
                old_session_id,
                new_session_id,
            } => serialize_session_fallback(old_session_id, new_session_id, serializer),
            Self::SessionInfoUpdate {
                session_id,
                title,
                updated_at,
            } => serialize_session_info_update(
                session_id,
                title.as_deref(),
                updated_at.as_deref(),
                serializer,
            ),
            Self::ElicitRequest(req) => serialize_elicit_request(req, serializer),
        }
    }
}

// Wire-shape tests moved to tests/services_acp_bridge_event_serialize.rs
// to keep this file within the 500-line monolith policy limit.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_variant_name_returns_name() {
        let cfg = AcpMcpServerConfig::Sse {
            name: "my-sse".to_string(),
            url: "http://localhost:3000/sse".to_string(),
            headers: vec![],
        };
        assert_eq!(cfg.name(), "my-sse");
    }

    #[test]
    fn http_variant_with_headers_roundtrips_serde() {
        let cfg = AcpMcpServerConfig::Http {
            name: "my-http".to_string(),
            url: "http://localhost:3000/mcp".to_string(),
            headers: vec![("Authorization".to_string(), "Bearer tok".to_string())],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, roundtrip);
    }

    #[test]
    fn sse_variant_roundtrips_serde() {
        let cfg = AcpMcpServerConfig::Sse {
            name: "my-sse".to_string(),
            url: "http://localhost:3000/sse".to_string(),
            headers: vec![("X-Api-Key".to_string(), "secret".to_string())],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, roundtrip);
    }
}
