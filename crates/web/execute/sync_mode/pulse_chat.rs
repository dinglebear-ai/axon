mod connection;
mod events;

use std::env;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::crates::core::config::Config;
use crate::crates::services::acp as acp_svc;
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::AcpPromptTurnRequest;

use connection::{fingerprint_mcp_servers, get_or_create_acp_connection};
use events::run_acp_event_loop;

/// Default per-turn timeout (5 minutes). Overridable via `AXON_ACP_TURN_TIMEOUT_MS`.
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Read the per-turn timeout from the environment, falling back to the default.
fn turn_timeout() -> Duration {
    env::var("AXON_ACP_TURN_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_TURN_TIMEOUT)
}

/// System prompt preamble injected into the first turn of an editor-integrated
/// Pulse session (i.e. when `session_id` is `None`).
///
/// # Divergence warning
///
/// The SSE path (`apps/web/app/api/pulse/chat/route.ts`) constructs its own
/// system prompt via `buildPulseSystemPrompt()` in TypeScript. These two
/// construction paths are intentionally separate due to transport-layer
/// differences (WS vs SSE), but their content must be kept in sync manually.
///
/// When updating this constant, also update `buildPulseSystemPrompt()` in
/// `apps/web/app/api/pulse/chat/route.ts` to reflect the same changes.
///
/// TODO: move to `crates/services/acp/` — tracked in WEB-INTEGRATION-REVIEW.md H-6
pub const AXON_EDITOR_SYSTEM_PROMPT_PREAMBLE: &str = "\
[System context — Axon editor integration]\n\
You have access to the user's Axon editor. To write content \
directly into the editor, output a block starting with the \
XML opening tag `<axon:editor op=\"replace\">` (or op=\"append\" \
to add to the end), followed by your markdown content, followed \
by the closing tag `</axon:editor>`. Do NOT show this tag in a \
code fence or explain it unless the user explicitly asks — \
just use it. The user will see the editor update in real time. \
Only use axon:editor tags when the user explicitly asks you to \
write to or update the editor.";

use super::super::events::CommandContext;
use super::super::mcp_config::read_axon_mcp_servers;
use super::acp_adapter::{AdapterCapabilities, resolve_acp_adapter_command};
use super::types::PulseChatAgent;

/// Build the session cache key for an ACP adapter.
///
/// The key encodes agent type, assistant mode, MCP config fingerprint, and
/// capability flags so that sessions with different configurations get
/// separate adapter processes.
pub(crate) fn build_agent_key(
    agent: PulseChatAgent,
    assistant_mode: bool,
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
    caps: &AdapterCapabilities,
) -> String {
    let mcp_fingerprint = fingerprint_mcp_servers(mcp_servers);
    let caps_fingerprint = format!(
        "fs={},term={},ptimeout={:?},atimeout={:?}",
        caps.enable_fs,
        caps.enable_terminal,
        caps.permission_timeout_secs,
        caps.adapter_timeout_secs,
    );
    if assistant_mode {
        format!("{agent:?}:assistant:mcp={mcp_fingerprint}:{caps_fingerprint}")
    } else {
        format!("{agent:?}:mcp={mcp_fingerprint}:{caps_fingerprint}")
    }
}

/// Handle the `pulse_chat` service mode: send a prompt turn to the persistent
/// ACP adapter and stream events back over the WS channel.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_pulse_chat(
    cfg: Arc<Config>,
    input: String,
    session_id: Option<String>,
    model: Option<String>,
    session_mode: Option<String>,
    enabled_mcp_servers: Option<Vec<String>>,
    blocked_mcp_tools: Vec<String>,
    agent: PulseChatAgent,
    assistant_mode: bool,
    caps: AdapterCapabilities,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    permission_responders: acp_svc::PermissionResponderMap,
) -> Result<(), String> {
    tracing::info!(
        context = "pulse_chat",
        agent = ?agent,
        assistant_mode,
        session_id = ?session_id,
        model = ?model,
        input_len = input.len(),
        "starting pulse_chat",
    );

    let mcp_servers = filter_mcp_servers(enabled_mcp_servers).await;

    let prompt_input = if session_id.is_none() {
        format!("{AXON_EDITOR_SYSTEM_PROMPT_PREAMBLE}\n[User message]\n{input}")
    } else {
        input
    };

    let req = AcpPromptTurnRequest {
        session_id,
        prompt: vec![prompt_input],
        model,
        session_mode,
        blocked_mcp_tools,
        mcp_servers,
    };

    let (agent_key, conn_handle) = get_or_create_acp_connection(
        &req,
        agent,
        assistant_mode,
        caps,
        &cfg,
        &permission_responders,
    )
    .await?;

    connection::execute_acp_turn(conn_handle, req, tx, ws_ctx, &agent_key).await
}

/// Read and filter MCP servers based on an optional allowlist.
async fn filter_mcp_servers(
    enabled_mcp_servers: Option<Vec<String>>,
) -> Vec<crate::crates::services::types::AcpMcpServerConfig> {
    let mut mcp_servers = read_axon_mcp_servers().await;
    if let Some(allowlist) = enabled_mcp_servers {
        let allowed: std::collections::HashSet<String> = allowlist.into_iter().collect();
        mcp_servers.retain(|server| allowed.contains(server.name()));
    }
    if !mcp_servers.is_empty() {
        tracing::info!(
            context = "pulse_chat",
            count = mcp_servers.len(),
            "passing MCP servers to ACP session",
        );
    }
    mcp_servers
}

/// Handle the `pulse_chat_probe` service mode.
pub(super) async fn handle_pulse_chat_probe(
    cfg: Arc<Config>,
    session_id: Option<String>,
    model: Option<String>,
    agent: PulseChatAgent,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    permission_responders: acp_svc::PermissionResponderMap,
) -> Result<(), String> {
    use crate::crates::services::types::AcpSessionProbeRequest;

    let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
    let adapter = resolve_acp_adapter_command(
        &cfg,
        agent,
        AdapterCapabilities {
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        },
    )?;
    let scaffold = acp_svc::AcpClientScaffold::new(adapter);
    let req = AcpSessionProbeRequest { session_id, model };
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    let task = tokio::spawn(async move {
        scaffold
            .start_session_probe(&req, cwd, Some(event_tx), permission_responders)
            .await
            .map_err(|e| e.to_string())
    });
    run_acp_event_loop(task, event_rx, tx, ws_ctx, "pulse_chat_probe")
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_agent_key_includes_agent_and_caps() {
        let key = build_agent_key(
            PulseChatAgent::Claude,
            false,
            &[],
            &AdapterCapabilities {
                enable_fs: true,
                enable_terminal: true,
                permission_timeout_secs: None,
                adapter_timeout_secs: None,
            },
        );
        assert!(key.starts_with("Claude:"));
        assert!(key.contains("fs=true"));
        assert!(key.contains("term=true"));
    }

    #[test]
    fn build_agent_key_assistant_mode_differs() {
        let caps = AdapterCapabilities {
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        };
        let normal = build_agent_key(PulseChatAgent::Claude, false, &[], &caps);
        let assistant = build_agent_key(PulseChatAgent::Claude, true, &[], &caps);
        assert_ne!(normal, assistant);
        assert!(assistant.contains(":assistant:"));
    }
}
