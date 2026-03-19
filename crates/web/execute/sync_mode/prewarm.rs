//! ACP adapter pre-warming — spawn default adapter on server boot.
//!
//! Eliminates the ~45-second cold start on the first chat message by
//! establishing the adapter session proactively during `start_server()`.
//!
//! **Cache key matching:** The prewarm uses default capabilities (fs=true,
//! term=true, no timeouts) and no MCP servers. This matches the most common
//! request configuration. Requests with different caps will cache-miss and
//! cold-start their own adapter — correct by design.
//!
//! **30-minute reaper window:** The pre-warmed session has the same idle TTL
//! as any other cached session (30 minutes). If no user sends a message
//! within that window, the warm adapter is reaped and the first request
//! will cold-start normally.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;

use crate::crates::core::config::Config;
use crate::crates::services::acp::{self as acp_svc, SESSION_CACHE};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::AcpPromptTurnRequest;

use super::acp_adapter::{AdapterCapabilities, resolve_acp_adapter_command};
use super::pulse_chat::build_agent_key;
use super::types::PulseChatAgent;

/// Prompt used for the session-setup probe (must be non-empty to pass validation).
const SESSION_SETUP_PROMPT: &str = "_";

/// Prompt for the warm-ping turn that forces `establish_acp_session()`.
const WARM_PING_PROMPT: &str = "Respond with exactly: WARM";

/// Maximum time to wait for the warm-ping turn to complete.
const PREWARM_TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Default capabilities used for pre-warmed sessions.
fn default_prewarm_caps() -> AdapterCapabilities {
    AdapterCapabilities {
        enable_fs: true,
        enable_terminal: true,
        permission_timeout_secs: None,
        adapter_timeout_secs: None,
    }
}

/// Returns a minimal `AcpPromptTurnRequest` for use in pre-warming.
///
/// The `prompt` arg differentiates the session-setup probe (`"_"`) from the
/// warm ping (`"Respond with exactly: WARM"`).
fn minimal_turn_req(prompt: &str) -> AcpPromptTurnRequest {
    AcpPromptTurnRequest {
        session_id: None,
        prompt: vec![prompt.to_string()],
        model: None,
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    }
}

/// Pre-warm a single ACP adapter by spawning it and sending a ping turn
/// to force session establishment.
async fn prewarm_adapter(cfg: &Arc<Config>, agent: PulseChatAgent) -> anyhow::Result<String> {
    let start = Instant::now();
    let caps = default_prewarm_caps();
    let agent_key = build_agent_key(agent, false, &[], &caps);

    // Skip if already cached.
    if SESSION_CACHE.get(&agent_key).is_some() {
        tracing::info!(
            context = "acp_prewarm",
            agent_key = %agent_key,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "adapter already cached — skipping prewarm",
        );
        return Ok(agent_key);
    }

    let adapter = resolve_acp_adapter_command(cfg, agent, caps)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("resolve adapter command")?;
    let adapter_name = std::path::Path::new(&adapter.program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&adapter.program)
        .to_string();

    tracing::info!(
        context = "acp_prewarm",
        program = %adapter_name,
        agent_key = %agent_key,
        "pre-warming adapter",
    );

    let scaffold = acp_svc::AcpClientScaffold::new(adapter.clone());
    let initialize = scaffold
        .prepare_initialize()
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prepare_initialize failed")?;
    let cwd = resolve_prewarm_working_dir().await?;

    // Use prepare_session_setup with a minimal AcpPromptTurnRequest.
    // Routes through the same code path as real requests.
    let minimal_req = minimal_turn_req(SESSION_SETUP_PROMPT);
    let session_setup = scaffold
        .prepare_session_setup(&minimal_req, &cwd)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prepare_session_setup failed")?;

    let permission_responders: acp_svc::PermissionResponderMap = Arc::new(dashmap::DashMap::new());

    let handle = Arc::new(acp_svc::AcpConnectionHandle::spawn(
        adapter,
        initialize,
        session_setup,
        permission_responders.clone(),
    ));

    // Send a lightweight ping turn to force establish_acp_session().
    // Insert into SESSION_CACHE *after* successful dispatch so a failed
    // dispatch never leaves a stale/broken session in the cache.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<ServiceEvent>(64);
    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

    handle
        .run_turn(acp_svc::TurnRequest {
            req: minimal_turn_req(WARM_PING_PROMPT),
            service_tx: Some(event_tx),
            result_tx,
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prewarm turn dispatch failed")?;

    // Dispatch succeeded — safe to cache the session now.
    SESSION_CACHE.insert(
        agent_key.clone(),
        Arc::clone(&handle),
        permission_responders,
    );

    if let Some(cached) = SESSION_CACHE.get_sync(&agent_key) {
        cached.mark_turn_started();
    }

    // Drain events so the adapter loop doesn't block.
    let drain_handle = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    // Wait for the turn to complete (this is the ~45s cold start).
    // Flatten timeout + oneshot errors into a single Result so
    // mark_turn_completed is always called regardless of outcome.
    let turn_result = match tokio::time::timeout(PREWARM_TURN_TIMEOUT, result_rx).await {
        Ok(Ok(inner)) => inner,
        Ok(Err(_)) => Err("prewarm turn result channel dropped".to_string()),
        Err(_) => Err(format!(
            "prewarm turn timed out after {}s",
            PREWARM_TURN_TIMEOUT.as_secs()
        )),
    };

    // Always mark the turn completed so the session is not permanently
    // flagged as "in-flight" and reaped by the hung-turn detector.
    if let Some(cached) = SESSION_CACHE.get_sync(&agent_key) {
        cached.mark_turn_completed();
    }

    // Wrap the drain-task await in a short timeout so a stuck event channel
    // cannot hold prewarm open indefinitely after the turn result resolves.
    match tokio::time::timeout(std::time::Duration::from_secs(5), drain_handle).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(
                context = "acp_prewarm",
                error = %e,
                "prewarm event drain task panicked",
            );
        }
        Err(_) => {
            tracing::warn!(
                context = "acp_prewarm",
                "prewarm event drain timed out after 5s — aborting drain task",
            );
        }
    }

    match turn_result {
        Ok(()) => {
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "adapter pre-warmed successfully",
            );
            Ok(agent_key)
        }
        Err(e) => {
            // Log with structured fields at the module level (agent_key, program,
            // elapsed_ms) so these are searchable in JSON logs, then propagate
            // as Err so the caller logs at warn level instead of info.
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                error = %e,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "prewarm turn failed (adapter may still be usable)",
            );
            anyhow::bail!("prewarm turn failed for {agent_key}: {e}")
        }
    }
}

/// Resolve the working directory for pre-warmed adapters.
async fn resolve_prewarm_working_dir() -> anyhow::Result<std::path::PathBuf> {
    let base = std::env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            tracing::warn!(
                context = "acp_prewarm",
                "neither AXON_DATA_DIR nor HOME set, falling back to /tmp",
            );
            "/tmp".to_string()
        });
        format!("{home}/.local/share")
    });
    let path = std::path::PathBuf::from(base).join("axon").join("prewarm");
    tokio::fs::create_dir_all(&path)
        .await
        .context("failed to create prewarm working dir")?;
    Ok(path)
}

/// Spawn a background task that pre-warms the default ACP adapter(s).
///
/// Called once during `start_server()`. Non-blocking.
/// Controlled by `Config::acp_prewarm` (env `AXON_ACP_PREWARM`, default: true).
pub(crate) fn spawn_prewarm_task(cfg: Arc<Config>) {
    if !cfg.acp_prewarm {
        tracing::info!(
            context = "acp_prewarm",
            "pre-warming disabled via AXON_ACP_PREWARM=false"
        );
        return;
    }

    tokio::spawn(async move {
        // Small delay to let the server bind first.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let start = Instant::now();
        match prewarm_adapter(&cfg, PulseChatAgent::Claude).await {
            Ok(key) => {
                tracing::info!(
                    context = "acp_prewarm",
                    agent_key = %key,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "prewarm complete",
                )
            }
            Err(e) => {
                tracing::warn!(
                    context = "acp_prewarm",
                    error = %e,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "prewarm failed (will cold-start on first request)",
                )
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prewarm_caps_are_permissive() {
        let caps = default_prewarm_caps();
        assert!(caps.enable_fs);
        assert!(caps.enable_terminal);
        assert!(caps.permission_timeout_secs.is_none());
        assert!(caps.adapter_timeout_secs.is_none());
    }

    #[test]
    fn default_config_has_prewarm_enabled() {
        let cfg = Config::default();
        assert!(
            cfg.acp_prewarm,
            "default config should have prewarm enabled"
        );
    }
}
