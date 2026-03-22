//! ACP (Agent Client Protocol) service layer.
//!
//! Provides `AcpClientScaffold` for spawning and communicating with ACP
//! adapter subprocesses (Claude CLI, Codex, Gemini).
//!
//! # Module layout
//!
//! - `adapters`  — adapter kind detection, model normalization, model override
//! - `bridge`    — `AcpBridgeClient` implementing the ACP SDK `Client` trait
//! - `config`    — config directory discovery and model file readers
//! - `mapping`   — ACP SDK type → service-layer type conversions + validators
//! - `runtime`   — `run_prompt_turn` / `run_session_probe` orchestration
//!
//! Callers outside this crate only need the public surface exported below.

pub mod adapters;
pub(super) mod bridge;
pub(super) mod config;
pub mod mapping;
pub(super) mod permission;
pub(super) mod persistent_conn;
pub(super) mod preflight;
pub(super) mod runtime;
pub(super) mod session;
pub mod session_cache;

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use agent_client_protocol::{
    InitializeRequest, LoadSessionRequest, NewSessionRequest, ProtocolVersion,
};
use tokio::sync::mpsc;

use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    AcpAdapterCommand, AcpPromptTurnRequest, AcpSessionProbeRequest,
};

use mapping::build_session_setup;
use runtime::{run_prompt_turn, run_session_probe};

// ── Public type re-exports ───────────────────────────────────────────────────

pub use bridge::AcpRuntimeState;
pub use mapping::{
    map_config_options, map_permission_request, map_permission_request_event,
    map_session_notification, map_session_notification_event, map_session_update_kind,
    validate_adapter_command, validate_probe_request, validate_prompt_turn_request,
    validate_session_cwd,
};
pub use persistent_conn::{AcpConnectionHandle, TurnRequest};
pub use session_cache::SESSION_CACHE;

// ── PermissionResponderMap ───────────────────────────────────────────────────

/// Shared map of pending permission responses keyed by `tool_call_id`.
///
/// FINDING-13: Uses `DashMap` instead of `Arc<Mutex<HashMap<...>>>` to eliminate
/// lock contention when the bridge inserts/removes permission responders from
/// concurrent async tasks. DashMap uses shard-level locking internally.
///
/// When a permission request arrives from the ACP agent, the bridge inserts a
/// oneshot sender here. The WS handler (or auto-approve logic) sends the chosen
/// `option_id` through it.
/// Key is `(session_id, tool_call_id)` to prevent cross-session collisions
/// (SEC-7): two concurrent sessions cannot accidentally receive each other's
/// permission responses even if their `tool_call_id` values happen to collide.
pub type PermissionResponderMap =
    Arc<dashmap::DashMap<(String, String), tokio::sync::oneshot::Sender<String>>>;

// ── AcpSessionSetupRequest ───────────────────────────────────────────────────

/// Discriminates between creating a new ACP session and loading an existing one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpSessionSetupRequest {
    New(NewSessionRequest),
    Load(LoadSessionRequest),
}

// ── AcpClientScaffold ───────────────────────────────────────────────────────

/// Minimal ACP client scaffold for the services layer.
///
/// Handles process lifecycle: spawn adapter, initialize ACP protocol, set up
/// session, run prompt turn or session probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpClientScaffold {
    adapter: AcpAdapterCommand,
}

const ACP_ADAPTER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Environment variables forwarded to ACP adapter subprocesses.
///
/// `env_clear()` strips everything; only these keys are re-injected from the
/// parent environment when present.  Kept as a module-level constant so
/// `spawn_adapter` and `spawn_adapter_skip_validation` stay in sync.
const ACP_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "TZ",
    "TMPDIR",
    "XDG_RUNTIME_DIR",
    // TLS certificate paths for custom CA bundles
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    // Gemini CLI auth and config
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GEMINI_CLI_HOME",
    "GEMINI_FORCE_FILE_STORAGE",
];

/// Clear all inherited env vars from `cmd`, then re-inject only the keys in
/// [`ACP_ENV_ALLOWLIST`] that are present in the parent environment.
fn apply_env_allowlist(cmd: &mut tokio::process::Command) {
    cmd.env_clear();
    for key in ACP_ENV_ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
}

// ── run_acp_event_loop ───────────────────────────────────────────────────────

/// Spawn a dedicated `spawn_blocking` thread with a `current_thread` tokio
/// runtime and `LocalSet`, run `fut` inside it with a 5-minute timeout, and
/// return the result.
///
/// ACP SDK futures are `!Send` (uses `?Send` traits), so they cannot be spawned
/// directly on the Axon multi-thread runtime.  This helper encapsulates that
/// boilerplate once so `start_prompt_turn` and `start_session_probe` do not
/// duplicate the identical setup.
async fn run_acp_event_loop<F, Fut>(
    timeout: std::time::Duration,
    fut: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), String>>,
{
    let join = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| format!("failed to create ACP tokio runtime: {err}"))?;
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            match tokio::time::timeout(timeout, fut()).await {
                Ok(result) => result,
                Err(_) => Err(format!(
                    "ACP adapter timed out after {} seconds",
                    timeout.as_secs()
                )),
            }
        })
    })
    .await
    .map_err(|err| format!("failed to join ACP runtime worker: {err}"))?;

    join.map_err(|err| err.to_string())?;
    Ok(())
}

impl AcpClientScaffold {
    #[must_use]
    pub fn new(adapter: AcpAdapterCommand) -> Self {
        Self { adapter }
    }

    #[must_use]
    pub fn adapter(&self) -> &AcpAdapterCommand {
        &self.adapter
    }

    pub fn validate_adapter(&self) -> Result<(), Box<dyn std::error::Error>> {
        validate_adapter_command(&self.adapter)
    }

    /// Build the adapter subprocess `Command` with common configuration:
    /// args, cwd, env allowlist, stdio pipes, and `kill_on_drop`.
    ///
    /// Shared by `spawn_adapter` and `spawn_adapter_skip_validation` to
    /// eliminate duplication.  `env_clear()` only affects the *subprocess*
    /// environment — the parent (Axon) process environment is never touched.
    fn build_adapter_command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.adapter.program);
        command.args(&self.adapter.args);
        if let Some(cwd) = &self.adapter.cwd {
            command.current_dir(cwd);
        }
        // Clear all inherited env vars, then allowlist only what adapters need.
        // OPENAI_* vars are intentionally excluded — they point at Axon's local LLM
        // proxy, not at OpenAI. Adapters (Claude CLI, Codex) use their own OAuth /
        // stored API keys for authentication.
        // CLAUDECODE is excluded to prevent nested-session detection.
        apply_env_allowlist(&mut command);
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        // Ensure the child is killed if its handle is dropped without explicit
        // cleanup — covers timeout paths where the outer future is cancelled.
        command.kill_on_drop(true);
        command
    }

    /// Spawn the adapter subprocess with a clean environment (env_clear allowlist).
    pub fn spawn_adapter(&self) -> Result<tokio::process::Child, Box<dyn std::error::Error>> {
        self.validate_adapter()?;
        if adapters::is_codex_adapter(&self.adapter) {
            match preflight::repair_codex_skill_symlinks() {
                Ok(Some(stats)) if stats.removed_dangling_symlinks > 0 => {
                    tracing::warn!(
                        context = "acp_preflight",
                        removed = stats.removed_dangling_symlinks,
                        scanned = stats.scanned_symlinks,
                        failed = stats.failed_removals,
                        "removed dangling skill symlink(s)"
                    );
                }
                Ok(Some(stats)) if stats.failed_removals > 0 => {
                    tracing::warn!(
                        context = "acp_preflight",
                        failed = stats.failed_removals,
                        scanned = stats.scanned_symlinks,
                        removed = stats.removed_dangling_symlinks,
                        "failed to inspect/remove skill symlink entry(ies)"
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(context = "acp_preflight", error = %err, "skill symlink repair failed");
                }
            }
        }
        let child = self.build_adapter_command().spawn()?;
        Ok(child)
    }

    /// Test-only variant of `spawn_adapter` that skips `validate_adapter_command`.
    ///
    /// Used by env-isolation integration tests that need to spawn a real shell
    /// (e.g. `sh`) without being blocked by the shell-name validator.
    #[cfg(any(test, feature = "test-helpers"))]
    #[doc(hidden)]
    pub fn spawn_adapter_skip_validation(
        &self,
    ) -> Result<tokio::process::Child, Box<dyn std::error::Error>> {
        let child = self.build_adapter_command().spawn()?;
        Ok(child)
    }

    pub fn prepare_initialize(&self) -> Result<InitializeRequest, Box<dyn std::error::Error>> {
        self.validate_adapter()?;
        let mut req = InitializeRequest::new(ProtocolVersion::V1).client_info(
            agent_client_protocol::Implementation::new("axon", env!("CARGO_PKG_VERSION")),
        );

        let mut caps = agent_client_protocol::ClientCapabilities::default();
        if self.adapter.enable_fs {
            caps.fs = agent_client_protocol::FileSystemCapabilities::new()
                .read_text_file(true)
                .write_text_file(true);
        }
        caps.terminal = self.adapter.enable_terminal;
        req.client_capabilities = caps;

        Ok(req)
    }

    pub fn prepare_session_setup(
        &self,
        req: &AcpPromptTurnRequest,
        cwd: impl AsRef<Path>,
    ) -> Result<AcpSessionSetupRequest, Box<dyn std::error::Error>> {
        self.validate_adapter()?;
        validate_prompt_turn_request(req)?;
        build_session_setup(req.session_id.as_deref(), cwd, &req.mcp_servers)
    }

    pub fn prepare_session_probe_setup(
        &self,
        req: &AcpSessionProbeRequest,
        cwd: impl AsRef<Path>,
    ) -> Result<AcpSessionSetupRequest, Box<dyn std::error::Error>> {
        self.validate_adapter()?;
        validate_probe_request(req)?;
        build_session_setup(req.session_id.as_deref(), cwd, &[])
    }

    pub async fn start_prompt_turn(
        &self,
        req: &AcpPromptTurnRequest,
        cwd: impl AsRef<Path>,
        tx: Option<mpsc::Sender<ServiceEvent>>,
        permission_responders: PermissionResponderMap,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let initialize = self.prepare_initialize()?;
        let session_setup = self.prepare_session_setup(req, cwd)?;
        emit(
            &tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: format!(
                    "ACP scaffold accepted prompt turn (session_id={})",
                    req.session_id.as_deref().unwrap_or("<new>")
                ),
            },
        )
        .await;

        let adapter = self.adapter.clone();
        let req_owned = req.clone();

        let timeout = self
            .adapter
            .adapter_timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(ACP_ADAPTER_TIMEOUT);

        // ACP SDK futures are !Send (uses ?Send traits) — delegate to the shared
        // `run_acp_event_loop` helper which provides a dedicated `current_thread`
        // runtime + LocalSet with a configurable timeout.
        run_acp_event_loop(timeout, move || {
            run_prompt_turn(
                adapter,
                initialize,
                session_setup,
                req_owned,
                tx,
                permission_responders,
            )
        })
        .await
    }

    pub async fn start_session_probe(
        &self,
        req: &AcpSessionProbeRequest,
        cwd: impl AsRef<Path>,
        tx: Option<mpsc::Sender<ServiceEvent>>,
        permission_responders: PermissionResponderMap,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let initialize = self.prepare_initialize()?;
        let session_setup = self.prepare_session_probe_setup(req, cwd)?;
        emit(
            &tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: format!(
                    "ACP scaffold accepted session probe (session_id={})",
                    req.session_id.as_deref().unwrap_or("<new>")
                ),
            },
        )
        .await;

        let adapter = self.adapter.clone();
        let req_owned = req.clone();

        let timeout = self
            .adapter
            .adapter_timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(ACP_ADAPTER_TIMEOUT);

        run_acp_event_loop(timeout, move || {
            run_session_probe(
                adapter,
                initialize,
                session_setup,
                req_owned,
                tx,
                permission_responders,
            )
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::preflight::repair_dangling_symlinks;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    #[cfg(unix)]
    fn removes_only_dangling_symlinks() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let existing_target = dir.join("existing-skill");
        fs::create_dir_all(&existing_target).expect("create existing target");
        let alive_link = dir.join("alive-link");
        symlink(&existing_target, &alive_link).expect("create alive symlink");
        let broken_link = dir.join("broken-link");
        symlink(dir.join("missing-target"), &broken_link).expect("create broken symlink");
        let regular = dir.join("regular-file");
        fs::write(&regular, "ok").expect("create regular file");

        let stats = repair_dangling_symlinks(dir).expect("repair");
        assert_eq!(stats.scanned_symlinks, 2);
        assert_eq!(stats.removed_dangling_symlinks, 1);
        assert_eq!(stats.failed_removals, 0);
        assert!(alive_link.exists());
        assert!(!broken_link.exists());
        assert!(regular.exists());
    }

    #[test]
    fn repair_dangling_symlinks_returns_error_for_missing_dir() {
        let tmp = tempdir().expect("tempdir");
        let missing = tmp.path().join("missing");
        assert!(repair_dangling_symlinks(&missing).is_err());
    }
}
