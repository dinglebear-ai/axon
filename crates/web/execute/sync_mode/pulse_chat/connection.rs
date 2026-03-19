//! ACP persistent connection management for Pulse chat sessions.
//!
//! Extracted from `pulse_chat.rs` to stay under the 500-line module limit.
//! Handles connection caching, adapter spawning, turn execution, and
//! working directory resolution.

use std::env;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use crate::crates::core::config::Config;
use crate::crates::core::paths::{axon_data_base_dir, path_basename};
use crate::crates::services::acp::{self as acp_svc, AcpConnectionHandle, SESSION_CACHE};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::AcpPromptTurnRequest;

use super::super::acp_adapter::{AdapterCapabilities, resolve_acp_adapter_command};
use super::super::types::PulseChatAgent;
use super::events::drive_turn_events;
use super::{build_agent_key, turn_timeout};

/// Get or create the persistent ACP adapter connection from the global cache.
///
/// If the requested agent+MCP config (agent_key) matches a cached entry, it is
/// reused. Otherwise a fresh adapter subprocess is spawned and cached.
pub(in crate::crates::web) async fn get_or_create_acp_connection(
    req: &AcpPromptTurnRequest,
    agent: PulseChatAgent,
    assistant_mode: bool,
    caps: AdapterCapabilities,
    cfg: &Arc<Config>,
    permission_responders: &acp_svc::PermissionResponderMap,
) -> Result<(String, Arc<AcpConnectionHandle>), String> {
    let agent_key = build_agent_key(agent, assistant_mode, &req.mcp_servers, &caps);

    // Check global cache first. If a turn has been in-flight longer than the
    // timeout threshold, the adapter is likely hung — evict and spawn fresh.
    if let Some(cached) = SESSION_CACHE.get(&agent_key) {
        if cached.is_turn_hung(turn_timeout()) {
            tracing::warn!(
                context = "pulse_chat",
                agent_key = %agent_key,
                "evicting cached session with hung turn — spawning fresh adapter",
            );
            SESSION_CACHE.remove(&agent_key);
        } else {
            return Ok((agent_key, Arc::clone(&cached.handle)));
        }
    }

    // Spawn a new adapter subprocess.
    let adapter = resolve_acp_adapter_command(cfg, agent, caps)?;
    let adapter_name = path_basename(&adapter.program, &adapter.program);
    tracing::info!(
        context = "pulse_chat",
        program = adapter_name,
        args = ?adapter.args,
        "spawning persistent adapter",
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

    SESSION_CACHE.insert(
        agent_key.clone(),
        Arc::clone(&handle),
        permission_responders.clone(),
    );
    Ok((agent_key, handle))
}

/// Dispatch a single ACP turn on a persistent connection and handle the result.
///
/// Sends the turn request to the adapter, drives the event loop until completion,
/// and evicts the session from cache on fatal adapter errors (channel closed,
/// adapter exited, turn timeout) while preserving it for recoverable per-turn
/// errors.
pub(in crate::crates::web) async fn execute_acp_turn(
    conn_handle: Arc<AcpConnectionHandle>,
    req: AcpPromptTurnRequest,
    tx: mpsc::Sender<String>,
    ws_ctx: super::super::super::events::CommandContext,
    agent_key: &str,
) -> Result<(), String> {
    let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
    let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

    // Record that a turn is now in-flight for liveness detection.
    if let Some(cached) = SESSION_CACHE.get_sync(agent_key) {
        cached.mark_turn_started();
    }

    let send_result = conn_handle
        .run_turn(acp_svc::TurnRequest {
            req,
            service_tx: Some(event_tx),
            result_tx,
        })
        .await;

    if let Err(ref err) = send_result {
        tracing::warn!(context = "acp", agent_key, error = %err, "session evicted from cache after turn error");
        SESSION_CACHE.remove(agent_key);
        return send_result;
    }

    let timeout = turn_timeout();
    let turn_result = match tokio::time::timeout(
        timeout,
        drive_turn_events(result_rx, event_rx, tx, ws_ctx, agent_key),
    )
    .await
    {
        Ok(result) => result,
        Err(_elapsed) => {
            tracing::error!(
                context = "acp",
                agent_key,
                timeout_secs = timeout.as_secs(),
                "turn timed out — evicting session from cache",
            );
            SESSION_CACHE.remove(agent_key);
            Err(format!(
                "ACP turn timed out after {} seconds",
                timeout.as_secs()
            ))
        }
    };

    // Record turn completion for liveness tracking.
    if let Some(cached) = SESSION_CACHE.get_sync(agent_key) {
        cached.mark_turn_completed();
    }

    classify_and_evict_on_fatal(agent_key, &turn_result);

    turn_result
}

/// Classify a turn error as fatal (adapter broken) vs recoverable (per-turn)
/// and evict the session from cache only for fatal errors.
fn classify_and_evict_on_fatal(agent_key: &str, turn_result: &Result<(), String>) {
    let Err(ref err) = *turn_result else {
        return;
    };
    let is_fatal = err.contains("channel closed")
        || err.contains("channel dropped")
        || err.contains("adapter exited")
        || err.contains("result unavailable after channel close");
    if is_fatal {
        tracing::warn!(
            context = "acp",
            agent_key,
            error = %err,
            "session evicted from cache after fatal adapter error",
        );
        SESSION_CACHE.remove(agent_key);
    } else {
        tracing::debug!(context = "acp", agent_key, error = %err, "turn error (adapter still healthy)");
    }
}

/// Resolve the working directory for the adapter subprocess.
pub(super) async fn resolve_working_dir(
    assistant_mode: bool,
) -> Result<std::path::PathBuf, String> {
    if assistant_mode {
        let path = axon_data_base_dir().join("axon").join("assistant");
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| format!("failed to create assistant dir: {e}"))?;
        Ok(path)
    } else {
        env::current_dir().map_err(|e| e.to_string())
    }
}

/// Compute a SHA-256 fingerprint of the MCP server configuration.
pub(super) fn fingerprint_mcp_servers(
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
) -> String {
    use sha2::{Digest, Sha256};
    let json = serde_json::to_string(mcp_servers).unwrap_or_default();
    let hash = Sha256::digest(json.as_bytes());
    format!("{hash:x}")
}
