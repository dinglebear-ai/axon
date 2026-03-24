//! ACP session helper functions for the runtime layer.
//!
//! Extracted from `runtime.rs` to respect the 500-line monolith limit.
//! Contains: process spawn/IO wiring, connection initialization, session
//! setup dispatch, and config-option/model-config application.

use crate::crates::services::events::{LogLevel, ServiceEvent, emit, emit_nonblocking};
use crate::crates::services::types::{AcpAdapterCommand, AcpBridgeEvent, AcpModeUpdate};
use agent_client_protocol::{
    Agent, ClientSideConnection, InitializeRequest, NewSessionRequest, SessionConfigKind,
    SessionConfigOptionCategory, SessionId, SessionModeState, SetSessionConfigOptionRequest,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use super::adapters::normalized_requested_model;
use super::bridge::{AcpBridgeClient, AcpRuntimeState};
use super::config::{read_codex_cached_model_options, read_gemini_cached_model_options};
use super::mapping::{map_config_options, select_options_contains_value};
use super::permission::resolve_acp_auto_approve;
use super::runtime::AdapterGuard;
use super::{AcpClientScaffold, AcpSessionSetupRequest, PermissionResponderMap};

// ── extract_session_modes ─────────────────────────────────────────────────────

/// Extract available mode IDs from a `NewSessionResponse` modes field.
///
/// Returns an empty `Vec` when `modes` is `None` (adapter does not advertise
/// mode support) so callers can skip the `ModeUpdate` emission cheaply.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn extract_session_modes(modes: Option<&SessionModeState>) -> Vec<String> {
    let Some(state) = modes else {
        return Vec::new();
    };
    state
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect()
}

// ── SpawnedAdapter ───────────────────────────────────────────────────────────

/// Intermediate result of spawning the adapter process and wiring up I/O.
pub(super) struct SpawnedAdapter {
    pub(super) stdin: tokio::process::ChildStdin,
    pub(super) stdout: tokio::process::ChildStdout,
    pub(super) exit_rx: tokio::sync::oneshot::Receiver<String>,
}

// ── spawn_adapter_with_io ────────────────────────────────────────────────────

/// Spawn the adapter subprocess, wire up stderr logging and the exit watcher.
pub(super) fn spawn_adapter_with_io(
    adapter: AcpAdapterCommand,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SpawnedAdapter, String> {
    let scaffold = AcpClientScaffold::new(adapter);
    let child = scaffold
        .spawn_adapter()
        .map_err(|err| format!("failed to spawn ACP adapter: {err}"))?;
    let mut guard = AdapterGuard::new(child);

    let inner = guard.0.as_mut().ok_or("adapter guard empty")?;
    let child_stdin = inner
        .stdin
        .take()
        .ok_or_else(|| "ACP adapter stdin unavailable".to_string())?;
    let child_stdout = inner
        .stdout
        .take()
        .ok_or_else(|| "ACP adapter stdout unavailable".to_string())?;
    let child_stderr = inner
        .stderr
        .take()
        .ok_or_else(|| "ACP adapter stderr unavailable".to_string())?;

    // Spawn stderr reader.
    let stderr_tx = tx.clone();
    tokio::task::spawn_local(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(child_stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // Known non-fatal SDK chatter for quota/rate telemetry.
                    let level = if trimmed.contains("Unexpected case:")
                        && trimmed.contains("\"rate_limit_event\"")
                    {
                        LogLevel::Info
                    } else {
                        LogLevel::Warn
                    };
                    // Fire-and-forget: the stderr reader must never block on
                    // a full service event channel — otherwise backpressure
                    // from the channel stalls the reader and the adapter
                    // subprocess hangs waiting for stderr to drain.
                    emit_nonblocking(
                        &stderr_tx,
                        ServiceEvent::Log {
                            level,
                            message: if trimmed.len() > 500 {
                                format!(
                                    "ACP adapter stderr: {}… (truncated, {} bytes total)",
                                    &trimmed[..500],
                                    trimmed.len()
                                )
                            } else {
                                format!("ACP adapter stderr: {trimmed}")
                            },
                        },
                    );
                }
            }
        }
    });

    // Disarm guard — hand child to exit watcher.
    let mut child = guard.take().ok_or("adapter guard empty after stdio take")?;
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<String>();
    tokio::task::spawn_local(async move {
        match child.wait().await {
            Ok(status) if !status.success() => {
                let _ = exit_tx.send(format!("ACP adapter exited with {status}"));
            }
            Err(err) => {
                let _ = exit_tx.send(format!("ACP adapter wait failed: {err}"));
            }
            Ok(_) => {
                // Clean exit (code 0): drop the sender so the receiver sees
                // a closed channel rather than an empty message.  This lets
                // `run_prompt_turn` distinguish a clean shutdown (Err variant
                // from a dropped channel) from a crash (Ok(non-empty msg)).
                drop(exit_tx);
            }
        }
    });

    Ok(SpawnedAdapter {
        stdin: child_stdin,
        stdout: child_stdout,
        exit_rx,
    })
}

// ── initialize_connection ────────────────────────────────────────────────────

/// Wire up the ACP bridge client, create the `ClientSideConnection`, and send
/// the initialize request.
pub(super) async fn initialize_connection(
    spawned: SpawnedAdapter,
    adapter_cmd: &AcpAdapterCommand,
    initialize: InitializeRequest,
    cwd: std::path::PathBuf,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: &PermissionResponderMap,
) -> Result<
    (
        ClientSideConnection,
        Arc<AcpRuntimeState>,
        tokio::sync::oneshot::Receiver<String>,
    ),
    String,
> {
    // FINDING-2: RefCell is intentionally !Send — safe because this code runs
    // exclusively on a current_thread tokio runtime inside a LocalSet.
    #[expect(clippy::arc_with_non_send_sync)]
    let runtime_state = Arc::new(AcpRuntimeState::default());
    runtime_state
        .permission_timeout_secs
        .set(adapter_cmd.permission_timeout_secs);
    let auto_approve = resolve_acp_auto_approve();
    let bridge = AcpBridgeClient {
        runtime_state: runtime_state.clone(),
        auto_approve,
        permission_responders: permission_responders.clone(),
        session_cwd: cwd,
        terminal_manager: std::rc::Rc::new(std::cell::RefCell::new(
            super::bridge::terminal::TerminalManager::new(),
        )),
    };

    let msg =
        format!("ACP runtime: transport ready, starting initialize (auto_approve={auto_approve})");
    crate::crates::core::logging::log_info(&msg);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    let compat_stdin = spawned.stdin.compat_write();
    let compat_stdout = spawned.stdout.compat();

    let (conn, io_task) =
        ClientSideConnection::new(bridge, compat_stdin, compat_stdout, move |task| {
            tokio::task::spawn_local(task);
        });

    let io_tx = tx.clone();
    tokio::task::spawn_local(async move {
        match io_task.await {
            Ok(()) => {
                emit(
                    &io_tx,
                    ServiceEvent::Log {
                        level: LogLevel::Info,
                        message: "ACP runtime: IO task completed".to_string(),
                    },
                )
                .await;
            }
            Err(err) => {
                emit(
                    &io_tx,
                    ServiceEvent::Log {
                        level: LogLevel::Warn,
                        message: format!("ACP runtime: IO task failed: {err}"),
                    },
                )
                .await;
            }
        }
    });

    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ACP runtime: sending initialize request".to_string(),
        },
    )
    .await;
    let resp = conn
        .initialize(initialize)
        .await
        .map_err(|err| err.to_string())?;
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ACP initialized with protocol {}", resp.protocol_version),
        },
    )
    .await;

    // Store adapter's MCP transport capabilities for session-setup filtering.
    runtime_state
        .mcp_http_supported
        .set(resp.agent_capabilities.mcp_capabilities.http);
    runtime_state
        .mcp_sse_supported
        .set(resp.agent_capabilities.mcp_capabilities.sse);
    // Store load_session support and prompt capabilities from InitializeResponse.
    runtime_state
        .load_session_supported
        .set(resp.agent_capabilities.load_session);
    if let Ok(json) = serde_json::to_string(&resp.agent_capabilities.prompt_capabilities) {
        *runtime_state.prompt_capabilities_json.borrow_mut() = Some(json);
    }
    // Default true: assume close_session is supported unless the adapter explicitly
    // advertises session_capabilities WITHOUT the close field (i.e., None). For the
    // POC we use true unconditionally — the call is best-effort and non-blocking.
    let _ = &resp.agent_capabilities.session_capabilities.close; // read field to avoid dead_code lint
    runtime_state.close_session_supported.set(true);

    // Authentication: if the adapter advertised auth methods, authenticate using
    // the first advertised method.  AXON_ACP_AUTH_TOKEN is required — when it is
    // missing or empty, emit an error event and return an explicit error rather
    // than proceeding without credentials (which would result in a confusing
    // downstream failure).
    if let Some(method) = resp.auth_methods.first() {
        let token_result = std::env::var("AXON_ACP_AUTH_TOKEN");
        let token = match token_result {
            Ok(t) if !t.is_empty() => t,
            _ => {
                let msg =
                    "ACP: adapter requires authentication but AXON_ACP_AUTH_TOKEN is not set; \
                           set this environment variable to a valid auth token and retry"
                        .to_string();
                emit(
                    tx,
                    ServiceEvent::Log {
                        level: LogLevel::Error,
                        message: msg.clone(),
                    },
                )
                .await;
                return Err(msg);
            }
        };
        let _ = token; // token validated above; AuthenticateRequest carries the method, not the token
        use agent_client_protocol::AuthenticateRequest;
        match conn
            .authenticate(AuthenticateRequest::new(method.id().clone()))
            .await
        {
            Ok(_) => {
                emit_nonblocking(
                    tx,
                    ServiceEvent::Log {
                        level: LogLevel::Info,
                        message: "ACP: authenticated successfully".to_string(),
                    },
                );
            }
            Err(err) => {
                tracing::warn!(context = "acp_session", "ACP authenticate failed: {err}");
            }
        }
    }

    Ok((conn, runtime_state, spawned.exit_rx))
}

// ── setup_session ────────────────────────────────────────────────────────────

/// Validate that a session CWD exists and is a directory.
///
/// Called at the `setup_session` boundary before forwarding the request to the
/// ACP adapter.  Catching an invalid CWD here produces a clear error; the
/// adapter would otherwise fail with an opaque protocol error or silently use
/// its default working directory.
fn validate_cwd_usable(cwd: &std::path::Path) -> Result<(), String> {
    if !cwd.exists() {
        return Err(format!("ACP session cwd does not exist: {}", cwd.display()));
    }
    if !cwd.is_dir() {
        return Err(format!(
            "ACP session cwd is not a directory: {}",
            cwd.display()
        ));
    }
    Ok(())
}

/// Dispatch the session setup request (new or load-with-fallback).
///
/// Validates that the CWD embedded in the setup request exists and is a
/// directory before forwarding to the adapter.  When `load_session_supported`
/// is `false` the `Load` variant is treated as a `New` session (falls back
/// immediately rather than attempting a `load_session` call the adapter would
/// reject).
pub(super) async fn setup_session(
    conn: &ClientSideConnection,
    session_setup: AcpSessionSetupRequest,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    load_session_supported: bool,
) -> Result<
    (
        SessionId,
        Option<Vec<agent_client_protocol::SessionConfigOption>>,
    ),
    String,
> {
    match session_setup {
        AcpSessionSetupRequest::New(new_session) => {
            validate_cwd_usable(&new_session.cwd)?;
            let msg = "ACP runtime: creating new session".to_string();
            crate::crates::core::logging::log_info(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: msg,
                },
            )
            .await;
            let r = conn
                .new_session(new_session)
                .await
                .map_err(|e| e.to_string())?;
            // Emit initial mode state immediately so the frontend has it at session
            // start rather than waiting for the first ConfigOptionUpdate stream event.
            if let Some(ref mode_state) = r.modes {
                emit(
                    tx,
                    ServiceEvent::AcpBridge {
                        event: AcpBridgeEvent::ModeUpdate(AcpModeUpdate {
                            session_id: r.session_id.0.to_string(),
                            current_mode_id: mode_state.current_mode_id.0.to_string(),
                        }),
                    },
                )
                .await;
            }
            Ok((r.session_id, r.config_options))
        }
        AcpSessionSetupRequest::Load(load_session) => {
            validate_cwd_usable(&load_session.cwd)?;
            // If the adapter does not support load_session, skip directly to new_session.
            if !load_session_supported {
                let msg = "ACP runtime: adapter does not support load_session, falling back to new_session".to_string();
                crate::crates::core::logging::log_warn(&msg);
                emit(
                    tx,
                    ServiceEvent::Log {
                        level: LogLevel::Warn,
                        message: msg,
                    },
                )
                .await;
                let mut fallback_req = NewSessionRequest::new(load_session.cwd);
                if !load_session.mcp_servers.is_empty() {
                    fallback_req = fallback_req.mcp_servers(load_session.mcp_servers);
                }
                let r = conn
                    .new_session(fallback_req)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok((r.session_id, r.config_options));
            }
            let msg = "ACP runtime: loading existing session".to_string();
            crate::crates::core::logging::log_info(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: msg,
                },
            )
            .await;
            let requested_id = load_session.session_id.clone();
            let fallback_cwd = load_session.cwd.clone();
            // Clone before load_session is consumed: fallback NewSession gets the same
            // (already capability-filtered) MCP servers as the failed Load request.
            let fallback_mcp_servers = load_session.mcp_servers.clone();
            match conn.load_session(load_session).await {
                Ok(r) => Ok((requested_id, r.config_options)),
                Err(err) => {
                    let msg = format!("ACP load_session failed, falling back: {err}");
                    crate::crates::core::logging::log_warn(&msg);
                    emit(
                        tx,
                        ServiceEvent::Log {
                            level: LogLevel::Warn,
                            message: msg,
                        },
                    )
                    .await;
                    let mut fallback_req = NewSessionRequest::new(fallback_cwd);
                    if !fallback_mcp_servers.is_empty() {
                        fallback_req = fallback_req.mcp_servers(fallback_mcp_servers);
                    }
                    let r = conn
                        .new_session(fallback_req)
                        .await
                        .map_err(|e| e.to_string())?;
                    emit(
                        tx,
                        ServiceEvent::AcpBridge {
                            event: AcpBridgeEvent::SessionFallback {
                                old_session_id: requested_id.0.to_string(),
                                new_session_id: r.session_id.0.to_string(),
                            },
                        },
                    )
                    .await;
                    // Emit initial mode state for the fallback session too.
                    if let Some(ref mode_state) = r.modes {
                        emit(
                            tx,
                            ServiceEvent::AcpBridge {
                                event: AcpBridgeEvent::ModeUpdate(AcpModeUpdate {
                                    session_id: r.session_id.0.to_string(),
                                    current_mode_id: mode_state.current_mode_id.0.to_string(),
                                }),
                            },
                        )
                        .await;
                    }
                    Ok((r.session_id, r.config_options))
                }
            }
        }
    }
}

// ── apply_config_and_model ───────────────────────────────────────────────────

/// Emit config options from the session setup response, then apply the
/// requested model config override if one was specified.
pub(super) async fn apply_config_and_model(
    conn: &ClientSideConnection,
    session_id: &SessionId,
    initial_config_options: Option<Vec<agent_client_protocol::SessionConfigOption>>,
    model: Option<&str>,
    codex_adapter: bool,
    gemini_adapter: bool,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<crate::crates::services::types::AcpConfigOption>, String> {
    let mut latest_config_options = Vec::new();
    let mapped = initial_config_options
        .as_ref()
        .map(|o| map_config_options(o));
    let sid = session_id.0.to_string();
    if let Some(ref opts) = mapped
        && !opts.is_empty()
    {
        latest_config_options = opts.clone();
        emit(
            tx,
            ServiceEvent::AcpBridge {
                event: AcpBridgeEvent::ConfigOptionsUpdate {
                    session_id: sid.clone(),
                    config_options: opts.clone(),
                },
            },
        )
        .await;
    } else if codex_adapter {
        if let Some(fb) = read_codex_cached_model_options(model).await {
            latest_config_options = fb.clone();
            emit(
                tx,
                ServiceEvent::AcpBridge {
                    event: AcpBridgeEvent::ConfigOptionsUpdate {
                        session_id: sid.clone(),
                        config_options: fb,
                    },
                },
            )
            .await;
        }
    } else if gemini_adapter && let Some(fb) = read_gemini_cached_model_options(model).await {
        latest_config_options = fb.clone();
        emit(
            tx,
            ServiceEvent::AcpBridge {
                event: AcpBridgeEvent::ConfigOptionsUpdate {
                    session_id: sid.clone(),
                    config_options: fb,
                },
            },
        )
        .await;
    }

    if let Some(req_model) = normalized_requested_model(model)
        && let Some(ref opts) = initial_config_options
        && let Some(updated) = apply_model_config(conn, session_id, opts, req_model, tx).await?
    {
        latest_config_options = updated;
    }

    Ok(latest_config_options)
}

/// Apply a model config option if the requested model is in the allowed values.
async fn apply_model_config(
    conn: &ClientSideConnection,
    session_id: &SessionId,
    config_options: &[agent_client_protocol::SessionConfigOption],
    requested_model: String,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Option<Vec<crate::crates::services::types::AcpConfigOption>>, String> {
    let model_config = config_options.iter().find(|opt| {
        opt.category
            .as_ref()
            .is_some_and(|c| matches!(c, SessionConfigOptionCategory::Model))
    });
    let Some(model_config) = model_config else {
        return Ok(None);
    };

    let value_allowed = match &model_config.kind {
        SessionConfigKind::Select(select) => {
            select_options_contains_value(&select.options, &requested_model)
        }
        _ => false,
    };

    if value_allowed {
        let msg = format!("ACP runtime: setting model to {requested_model}");
        crate::crates::core::logging::log_info(&msg);
        emit(
            tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: msg,
            },
        )
        .await;
        let set_resp = conn
            .set_session_config_option(SetSessionConfigOptionRequest::new(
                session_id.clone(),
                model_config.id.clone(),
                requested_model.as_str(),
            ))
            .await
            .map_err(|err| format!("failed to set ACP model config: {err}"))?;
        let updated = map_config_options(&set_resp.config_options);
        if !updated.is_empty() {
            emit(
                tx,
                ServiceEvent::AcpBridge {
                    event: AcpBridgeEvent::ConfigOptionsUpdate {
                        session_id: session_id.0.to_string(),
                        config_options: updated.clone(),
                    },
                },
            )
            .await;
        }
        return Ok(Some(updated));
    } else {
        // Use the already-computed `requested_model` rather than recomputing
        // `normalized_requested_model(model)` — they produce the same value.
        let msg = format!("ACP runtime: skipping unsupported model value '{requested_model}'");
        crate::crates::core::logging::log_warn(&msg);
        emit(
            tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: msg,
            },
        )
        .await;
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    // Full session integration requires a live adapter process.
    // This test validates that build_session_setup preserves MCP servers
    // in the Load case (which is what the fallback path clones from).
    use super::super::mapping::build_session_setup;
    use crate::crates::services::types::AcpMcpServerConfig;

    #[test]
    fn test_modes_models_at_session_start() {
        // GREEN: extract_session_modes extracts available mode IDs from a
        // NewSessionResponse modes field so the frontend has them at session start.
        use agent_client_protocol::{SessionMode, SessionModeId, SessionModeState};
        let mode_state = SessionModeState::new(
            SessionModeId::new("default"),
            vec![SessionMode::new(
                SessionModeId::new("default"),
                "Default".to_string(),
            )],
        );
        let modes = super::extract_session_modes(Some(&mode_state));
        assert!(
            !modes.is_empty(),
            "should extract at least one mode from NewSessionResponse"
        );
        assert_eq!(modes[0], "default");
        // Verify None input returns empty vec (adapter with no mode support).
        let empty = super::extract_session_modes(None);
        assert!(empty.is_empty(), "None modes should produce empty vec");
    }

    #[test]
    fn build_session_setup_returns_load_variant_with_mcp_servers() {
        let cwd = std::env::temp_dir();
        let servers = vec![AcpMcpServerConfig::Stdio {
            name: "test-srv".into(),
            command: "/bin/echo".into(),
            args: vec![],
            env: vec![],
        }];
        let setup = build_session_setup(Some("existing-session"), &cwd, &servers)
            .expect("build_session_setup failed");
        match setup {
            super::super::AcpSessionSetupRequest::Load(req) => {
                assert_eq!(req.mcp_servers.len(), 1);
            }
            _ => panic!("expected Load variant"),
        }
    }
}
