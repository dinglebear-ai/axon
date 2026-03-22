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

/// Typed error for ACP turn failures, replacing substring matching with
/// pattern matching for fatal-vs-recoverable classification.
#[derive(Debug, thiserror::Error)]
pub(super) enum AcpTurnError {
    #[error("adapter channel closed")]
    ChannelClosed,
    #[error("adapter channel dropped")]
    ChannelDropped,
    #[error("adapter exited unexpectedly")]
    AdapterExited,
    #[error("result unavailable after channel close")]
    ResultUnavailable,
    #[error("turn timed out after {0} seconds")]
    Timeout(u64),
    #[error("{0}")]
    Other(String),
}

impl AcpTurnError {
    /// Fatal errors indicate the adapter subprocess is broken and the session
    /// must be evicted from cache. Recoverable errors are per-turn failures
    /// that leave the adapter healthy for subsequent turns.
    fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::ChannelClosed
                | Self::ChannelDropped
                | Self::AdapterExited
                | Self::ResultUnavailable
        )
    }

    /// Classify a string error (from the adapter layer) into a typed variant.
    fn from_turn_error(err: String) -> Self {
        if err.contains("channel closed") {
            Self::ChannelClosed
        } else if err.contains("channel dropped") {
            Self::ChannelDropped
        } else if err.contains("adapter exited") {
            Self::AdapterExited
        } else if err.contains("result unavailable after channel close") {
            Self::ResultUnavailable
        } else {
            Self::Other(err)
        }
    }
}

/// Per-key mutex map to prevent concurrent adapter spawns for the same
/// `agent_key`. The outer `std::sync::Mutex` protects the map itself (held
/// only briefly to insert/get); the inner `tokio::sync::Mutex` serializes
/// the async spawn logic per key.
static SPAWN_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Get or create the persistent ACP adapter connection from the global cache.
///
/// Uses a per-key mutex to guarantee at most one adapter spawn per `agent_key`
/// under concurrent load. The DashMap shard lock is never held across `.await`.
pub(in crate::crates::web) async fn get_or_create_acp_connection(
    req: &AcpPromptTurnRequest,
    agent: PulseChatAgent,
    assistant_mode: bool,
    caps: AdapterCapabilities,
    cfg: &Arc<Config>,
    permission_responders: &acp_svc::PermissionResponderMap,
) -> Result<(String, Arc<AcpConnectionHandle>), String> {
    let agent_key = build_agent_key(agent, assistant_mode, &req.mcp_servers, &caps);

    // Fast path: check cache without holding the spawn lock.
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

    // Acquire per-key mutex to serialize spawn attempts for this agent_key.
    let key_lock = {
        let mut locks = SPAWN_LOCKS.lock().expect("spawn_locks poisoned");
        Arc::clone(
            locks
                .entry(agent_key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    let _guard = key_lock.lock().await;

    // Re-check cache after acquiring the lock — another caller may have
    // already spawned the adapter while we waited.
    if let Some(cached) = SESSION_CACHE.get(&agent_key) {
        if !cached.is_turn_hung(turn_timeout()) {
            return Ok((agent_key, Arc::clone(&cached.handle)));
        }
        SESSION_CACHE.remove(&agent_key);
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

/// RAII guard that calls `mark_turn_completed` on drop, ensuring liveness
/// tracking is always cleaned up even if the turn panics or returns early.
struct TurnLivenessGuard {
    agent_key: String,
}

impl TurnLivenessGuard {
    fn start(agent_key: &str) -> Self {
        if let Some(cached) = SESSION_CACHE.get_sync(agent_key) {
            cached.mark_turn_started();
        }
        Self {
            agent_key: agent_key.to_string(),
        }
    }
}

impl Drop for TurnLivenessGuard {
    fn drop(&mut self) {
        if let Some(cached) = SESSION_CACHE.get_sync(&self.agent_key) {
            cached.mark_turn_completed();
        }
    }
}

/// Dispatch a single ACP turn on a persistent connection and handle the result.
///
/// Sends the turn request to the adapter, drives the event loop until completion,
/// and evicts the session from cache on fatal adapter errors (channel closed,
/// adapter exited, turn timeout) while preserving it for recoverable per-turn
/// errors.
///
/// Turn liveness tracking (`mark_turn_started`/`mark_turn_completed`) is managed
/// via a RAII guard, guaranteeing completion is always recorded regardless of the
/// turn outcome.
pub(in crate::crates::web) async fn execute_acp_turn(
    conn_handle: Arc<AcpConnectionHandle>,
    req: AcpPromptTurnRequest,
    tx: mpsc::Sender<String>,
    ws_ctx: super::super::super::events::CommandContext,
    agent_key: &str,
) -> Result<(), String> {
    let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
    let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

    // RAII guard: mark_turn_started now, mark_turn_completed on drop.
    let _liveness = TurnLivenessGuard::start(agent_key);

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
            let err = AcpTurnError::Timeout(timeout.as_secs());
            tracing::error!(
                context = "acp",
                agent_key,
                timeout_secs = timeout.as_secs(),
                "turn timed out — evicting session from cache",
            );
            SESSION_CACHE.remove(agent_key);
            Err(err.to_string())
        }
    };

    classify_and_evict_on_fatal(agent_key, &turn_result);

    turn_result
}

/// Classify a turn error as fatal (adapter broken) vs recoverable (per-turn)
/// and evict the session from cache only for fatal errors.
fn classify_and_evict_on_fatal(agent_key: &str, turn_result: &Result<(), String>) {
    let Err(ref err) = *turn_result else {
        return;
    };
    let typed = AcpTurnError::from_turn_error(err.clone());
    if typed.is_fatal() {
        tracing::warn!(
            context = "acp",
            agent_key,
            error = %typed,
            "session evicted from cache after fatal adapter error",
        );
        SESSION_CACHE.remove(agent_key);
    } else {
        tracing::debug!(context = "acp", agent_key, error = %typed, "turn error (adapter still healthy)");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_closed_is_fatal() {
        assert!(AcpTurnError::ChannelClosed.is_fatal());
    }

    #[test]
    fn channel_dropped_is_fatal() {
        assert!(AcpTurnError::ChannelDropped.is_fatal());
    }

    #[test]
    fn adapter_exited_is_fatal() {
        assert!(AcpTurnError::AdapterExited.is_fatal());
    }

    #[test]
    fn result_unavailable_is_fatal() {
        assert!(AcpTurnError::ResultUnavailable.is_fatal());
    }

    #[test]
    fn timeout_is_not_fatal() {
        assert!(!AcpTurnError::Timeout(300).is_fatal());
    }

    #[test]
    fn other_error_is_not_fatal() {
        assert!(!AcpTurnError::Other("some error".into()).is_fatal());
    }

    #[test]
    fn from_turn_error_classifies_channel_closed() {
        let err = AcpTurnError::from_turn_error("ACP adapter channel closed".into());
        assert!(matches!(err, AcpTurnError::ChannelClosed));
        assert!(err.is_fatal());
    }

    #[test]
    fn from_turn_error_classifies_adapter_exited() {
        let err = AcpTurnError::from_turn_error("adapter exited unexpectedly".into());
        assert!(matches!(err, AcpTurnError::AdapterExited));
        assert!(err.is_fatal());
    }

    #[test]
    fn from_turn_error_classifies_result_unavailable() {
        let err = AcpTurnError::from_turn_error("result unavailable after channel close".into());
        assert!(matches!(err, AcpTurnError::ResultUnavailable));
        assert!(err.is_fatal());
    }

    #[test]
    fn from_turn_error_classifies_unknown_as_other() {
        let err = AcpTurnError::from_turn_error("something unexpected happened".into());
        assert!(matches!(err, AcpTurnError::Other(_)));
        assert!(!err.is_fatal());
    }
}
