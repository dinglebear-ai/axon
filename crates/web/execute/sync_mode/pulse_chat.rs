use std::env;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use crate::crates::core::config::Config;
use crate::crates::services::acp::{self as acp_svc, AcpConnectionHandle, SESSION_CACHE};
use crate::crates::services::events::{LogLevel, ServiceEvent};
use crate::crates::services::types::{AcpBridgeEvent, AcpPromptTurnRequest};

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

use super::super::events::{CommandContext, acp_bridge_event_json, serialize_raw_output_event};
use super::super::mcp_config::read_axon_mcp_servers;
use super::acp_adapter::{AdapterCapabilities, resolve_acp_adapter_command};
use super::service_calls::send_json_owned;
use super::types::PulseChatAgent;

/// Send a single ACP `ServiceEvent` to the WS channel.
///
/// When the WS is disconnected (`tx.send()` fails), the serialized message is
/// buffered in the global session cache so it can be replayed on reconnect.
async fn dispatch_acp_event(
    event: ServiceEvent,
    tx: &mpsc::Sender<String>,
    ws_ctx: &CommandContext,
    agent_key: &str,
) {
    match event {
        ServiceEvent::Log { level, message } => {
            let truncated: String = message.chars().take(200).collect();
            match level {
                LogLevel::Info => log::info!("[pulse_chat] {truncated}"),
                LogLevel::Warn => log::warn!("[pulse_chat] {truncated}"),
                LogLevel::Error => log::error!("[pulse_chat] {truncated}"),
            }
            send_json_owned(
                tx.clone(),
                ws_ctx.clone(),
                json!({"type": "status", "level": level, "message": message}),
            )
            .await;
        }
        ServiceEvent::AcpBridge { event } => {
            // Capture session_id from TurnResult for the session cache index.
            if let AcpBridgeEvent::TurnResult(ref result) = event {
                SESSION_CACHE.register_session_id(result.session_id.clone(), agent_key.to_string());
            }
            let raw_json = acp_bridge_event_json(&event);
            let event_type = raw_json
                .strip_prefix(r#"{"type":""#)
                .and_then(|rest| rest.find('"').map(|e| &rest[..e]));
            if !matches!(
                event_type,
                Some("assistant_delta") | Some("thinking_content") | Some("user_delta")
            ) {
                log::info!(
                    "[pulse_chat] ACP event: type={}",
                    event_type.unwrap_or("unknown")
                );
            }
            if let Some(envelope) = serialize_raw_output_event(ws_ctx, &raw_json) {
                send_or_buffer(tx, envelope, agent_key).await;
            }
        }
        ServiceEvent::EditorWrite { content, operation } => {
            log::info!(
                "[pulse_chat] editor_update: op={operation} content_len={}",
                content.len()
            );
            let standalone = json!({
                "type": "editor_update",
                "content": content,
                "operation": operation,
            })
            .to_string();
            send_or_buffer(tx, standalone, agent_key).await;
        }
    }
}

/// Try to send a WS message. On failure (WS disconnected), buffer it in the
/// global session cache so it can be replayed when the client reconnects.
async fn send_or_buffer(tx: &mpsc::Sender<String>, msg: String, agent_key: &str) {
    if tx.send(msg.clone()).await.is_err()
        && let Some(cached) = SESSION_CACHE.get_sync(agent_key)
    {
        cached.buffer_event(msg);
    }
}

/// Drive the ACP event loop for a persistent-connection turn.
///
/// Polls `result_rx` and `event_rx` concurrently; forwards each `ServiceEvent`
/// to the WS channel as it arrives. Returns after the result is received and
/// the event channel is drained.
async fn drive_turn_events(
    mut result_rx: oneshot::Receiver<Result<(), String>>,
    mut event_rx: mpsc::Receiver<ServiceEvent>,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    agent_key: &str,
) -> Result<(), String> {
    loop {
        tokio::select! {
            biased;
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(event) => dispatch_acp_event(event, &tx, &ws_ctx, agent_key).await,
                    None => {
                        let run_result = result_rx
                            .try_recv()
                            .map_err(|_| "ACP turn result unavailable after channel close")?;
                        return run_result;
                    }
                }
            }
            result = &mut result_rx => {
                let run_result = result.map_err(|_| "ACP turn result channel dropped".to_string())?;
                while let Ok(event) = event_rx.try_recv() {
                    dispatch_acp_event(event, &tx, &ws_ctx, agent_key).await;
                }
                return run_result;
            }
        }
    }
}

/// Get or create the persistent ACP adapter connection from the global cache.
///
/// If the requested agent+MCP config (agent_key) matches a cached entry, it is
/// reused. Otherwise a fresh adapter subprocess is spawned and cached.
async fn get_or_create_acp_connection(
    req: &AcpPromptTurnRequest,
    agent: PulseChatAgent,
    assistant_mode: bool,
    caps: AdapterCapabilities,
    cfg: &Arc<Config>,
    permission_responders: &acp_svc::PermissionResponderMap,
) -> Result<(String, Arc<AcpConnectionHandle>), String> {
    let mcp_fingerprint = fingerprint_mcp_servers(&req.mcp_servers);
    let agent_key = if assistant_mode {
        format!("{agent:?}:assistant:mcp={mcp_fingerprint}")
    } else {
        format!("{agent:?}:mcp={mcp_fingerprint}")
    };

    // Check global cache first.
    if let Some(cached) = SESSION_CACHE.get(&agent_key) {
        return Ok((agent_key, Arc::clone(&cached.handle)));
    }

    // Spawn a new adapter subprocess.
    let adapter = resolve_acp_adapter_command(cfg, agent, caps)?;
    let adapter_name = std::path::Path::new(&adapter.program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&adapter.program);
    log::info!(
        "[pulse_chat] spawning persistent adapter: program={adapter_name} args={:?}",
        adapter.args
    );
    let scaffold = acp_svc::AcpClientScaffold::new(adapter.clone());
    let initialize = scaffold.prepare_initialize().map_err(|e| e.to_string())?;
    let cwd = resolve_working_dir(assistant_mode).await?;
    let session_setup = scaffold
        .prepare_session_setup(req, cwd)
        .map_err(|e| e.to_string())?;

    let handle = Arc::new(AcpConnectionHandle::spawn(
        adapter,
        initialize,
        session_setup,
        permission_responders.clone(),
    ));

    let cached = SESSION_CACHE.insert(
        agent_key.clone(),
        Arc::clone(&handle),
        permission_responders.clone(),
    );
    let _ = cached; // ensure insert result is used (for reaper start)
    Ok((agent_key, handle))
}

/// Resolve the working directory for the adapter subprocess.
async fn resolve_working_dir(assistant_mode: bool) -> Result<std::path::PathBuf, String> {
    if assistant_mode {
        let base = env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            format!("{home}/.local/share")
        });
        let path = std::path::PathBuf::from(base)
            .join("axon")
            .join("assistant");
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| format!("failed to create assistant dir: {e}"))?;
        Ok(path)
    } else {
        env::current_dir().map_err(|e| e.to_string())
    }
}

fn fingerprint_mcp_servers(
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
) -> String {
    serde_json::to_string(mcp_servers).unwrap_or_default()
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
    log::info!(
        "[pulse_chat] starting: agent={:?} assistant_mode={} session_id={:?} model={:?} input_len={}",
        agent,
        assistant_mode,
        session_id,
        model,
        input.len()
    );

    let mut mcp_servers = read_axon_mcp_servers().await;
    if let Some(allowlist) = enabled_mcp_servers {
        let allowed: std::collections::HashSet<String> = allowlist.into_iter().collect();
        mcp_servers.retain(|server| match server {
            crate::crates::services::types::AcpMcpServerConfig::Stdio { name, .. } => {
                allowed.contains(name)
            }
            crate::crates::services::types::AcpMcpServerConfig::Http { name, .. } => {
                allowed.contains(name)
            }
        });
    }
    if !mcp_servers.is_empty() {
        log::info!(
            "[pulse_chat] passing {} MCP server(s) to ACP session",
            mcp_servers.len()
        );
    }

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

    execute_acp_turn(conn_handle, req, tx, ws_ctx, &agent_key).await
}

/// Dispatch a single ACP turn on a persistent connection and handle the result.
///
/// Sends the turn request to the adapter, drives the event loop until completion,
/// and evicts the session from cache on fatal adapter errors (channel closed,
/// adapter exited) while preserving it for recoverable per-turn errors.
async fn execute_acp_turn(
    conn_handle: Arc<AcpConnectionHandle>,
    req: AcpPromptTurnRequest,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    agent_key: &str,
) -> Result<(), String> {
    let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
    let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

    let send_result = conn_handle
        .run_turn(acp_svc::TurnRequest {
            req,
            service_tx: Some(event_tx),
            result_tx,
        })
        .await;

    if let Err(ref err) = send_result {
        // The adapter channel is closed — the subprocess died before we could
        // dispatch this turn. Evict immediately so the next call spawns a fresh
        // adapter rather than retrying against the same dead handle for up to
        // 30 minutes.
        log::warn!("[acp] session {agent_key} evicted from cache after turn error: {err}");
        SESSION_CACHE.remove(agent_key);
        return send_result;
    }

    let turn_result = drive_turn_events(result_rx, event_rx, tx, ws_ctx, agent_key).await;

    if let Err(ref err) = turn_result {
        // Only evict if the error indicates the adapter process/channel is
        // broken.  Per-turn errors (content policy, tool errors, timeouts)
        // leave the persistent adapter healthy — evicting on those wastes the
        // warm subprocess and forces a cold spawn on the next turn.
        let is_fatal = err.contains("channel closed")
            || err.contains("channel dropped")
            || err.contains("adapter exited")
            || err.contains("result unavailable after channel close");
        if is_fatal {
            log::warn!(
                "[acp] session {agent_key} evicted from cache after fatal adapter error: {err}"
            );
            SESSION_CACHE.remove(agent_key);
        } else {
            log::debug!("[acp] session {agent_key} turn error (adapter still healthy): {err}");
        }
    }

    turn_result
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

/// Drive the ACP event loop for a non-persistent path (pulse_chat_probe).
async fn run_acp_event_loop(
    mut task: tokio::task::JoinHandle<Result<(), String>>,
    mut event_rx: mpsc::Receiver<ServiceEvent>,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    task_name: &'static str,
) -> Result<Option<String>, String> {
    loop {
        tokio::select! {
            biased;
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(event) => dispatch_acp_event(event, &tx, &ws_ctx, "").await,
                    None => {
                        let run_result = (&mut task)
                            .await
                            .map_err(|e| format!("failed to join {task_name} task: {e}"))?;
                        run_result?;
                        break;
                    }
                }
            }
            join_result = &mut task => {
                let run_result = join_result
                    .map_err(|e| format!("failed to join {task_name} task: {e}"))?;
                run_result?;
                while let Ok(event) = event_rx.try_recv() {
                    dispatch_acp_event(event, &tx, &ws_ctx, "").await;
                }
                break;
            }
        }
    }
    Ok(None)
}
