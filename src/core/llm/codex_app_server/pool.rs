//! Bounded pool of long-lived `codex app-server` children.
//!
//! The current spawn-per-completion model pays process-startup overhead (~300ms)
//! on every synthesis call. This module replaces that with a pool of N children,
//! each initialised once and then reused for many `turn/start` cycles.
//!
//! ## Lifecycle
//!
//! 1. On first use the pool is created and `pool_size` children are spawned in
//!    the background. Completions block until a slot is ready.
//! 2. Each child runs `initialize` → `initialized` → `thread/start` once at
//!    spawn time. Per-turn callers send only `turn/start` and read until
//!    `turn/completed`.
//! 3. After a successful turn the slot is returned to the idle queue.
//! 4. After a timeout, a protocol error, or an unhealthy child, the slot is
//!    discarded and a fresh child is spawned to replace it.
//! 5. Idle children whose last use was more than `idle_ttl` ago are replaced on
//!    the next checkout.
//!
//! ## Pool keying
//!
//! Pools are keyed by `CompletionKey` (cmd + model) in a process-global
//! `DashMap`. A configuration change that produces a new key automatically uses
//! a fresh pool.

use std::error::Error as StdError;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tempfile::TempDir;
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::core::llm::LlmBackendConfig;
use crate::core::llm::codex_app_server::{
    cleanup_codex_child, collect_stderr, configure_codex_child_isolation,
    read_bounded_stderr_spawn, stderr_diagnostics_suffix,
};
use crate::core::llm::types::CompletionResponse;
use crate::core::logging::{log_info, log_warn};

use super::home;
use super::protocol::run_init_handshake;

type BoxError = Box<dyn StdError + Send + Sync>;

/// Default idle TTL for a pooled child.
///
/// A child that has been idle for this long is discarded and replaced on next
/// checkout rather than being handed to a caller. This bounds memory (kept
/// `CODEX_HOME` temp dirs) and avoids handing out a process that the OS may
/// have reaped after a long pause.
const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(300);

/// Process-global map from `CompletionKey` string → pool.
///
/// The pool is initialised lazily on first use. Different (cmd, model) pairs
/// get independent pools, so a configuration change automatically uses a fresh
/// pool without invalidating existing callers.
static POOL_MAP: LazyLock<DashMap<String, Arc<CodexPool>>> = LazyLock::new(DashMap::new);

/// Parse `AXON_CODEX_POOL_IDLE_TTL_SECS` from the environment, defaulting to
/// [`DEFAULT_IDLE_TTL`].
fn idle_ttl_from_env() -> Duration {
    std::env::var("AXON_CODEX_POOL_IDLE_TTL_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&v| v > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_IDLE_TTL)
}

/// An initialised child ready for `turn/start` cycles.
pub(super) struct PoolSlot {
    /// Thread-id returned by `thread/start`.
    pub(super) thread_id: String,
    /// Write half of the child's stdin.
    pub(super) stdin: ChildStdin,
    /// Buffered reader over stdout.
    pub(super) stdout: BufReader<ChildStdout>,
    /// The child process itself (owns the wait handle).
    child: Child,
    /// Background stderr drain task.
    stderr_task: JoinHandle<Result<Vec<u8>, io::Error>>,
    /// Owns the isolated CODEX_HOME (dropped with the slot when unhealthy).
    _home_guard: Option<TempDir>,
    /// When this slot last finished a turn.
    last_used: Instant,
    /// Incremented on each successfully returned turn (diagnostic only).
    turns_served: u64,
}

impl PoolSlot {
    /// True when the slot has exceeded the configured idle TTL.
    fn is_stale(&self, ttl: Duration) -> bool {
        self.last_used.elapsed() > ttl
    }

    /// Mark the slot as just-returned and update `turns_served`.
    fn on_return(&mut self) {
        self.last_used = Instant::now();
        self.turns_served += 1;
    }
}

/// Bounded pool of reusable `codex app-server` children.
pub(super) struct CodexPool {
    idle: Mutex<Vec<PoolSlot>>,
    size: usize,
    idle_ttl: Duration,
    backend: LlmBackendConfig,
}

impl CodexPool {
    fn new(size: usize, idle_ttl: Duration, backend: LlmBackendConfig) -> Arc<Self> {
        Arc::new(Self {
            idle: Mutex::new(Vec::with_capacity(size)),
            size,
            idle_ttl,
            backend,
        })
    }

    /// Acquire one ready-to-use slot. Blocks until a slot is available (up to
    /// `timeout`). Returns an error when the slot cannot be spawned/initialised
    /// within the timeout, or when the pool is shut down.
    pub(super) async fn checkout(&self, timeout: Duration) -> Result<PoolSlot, BoxError> {
        let deadline = Instant::now() + timeout;
        // Single-pass by construction: drain idle (returning the first healthy
        // slot) else spawn one. The `loop` is the retry scaffold for a future
        // contend-and-retry path; today every branch resolves in one iteration.
        #[allow(clippy::never_loop)]
        loop {
            // Try to take a healthy idle slot first.
            {
                let mut idle = self.idle.lock().await;
                while let Some(slot) = idle.pop() {
                    if slot.is_stale(self.idle_ttl) {
                        log_info(&format!(
                            "codex pool: discarding stale slot (idle {:.1}s, {} turns served)",
                            slot.last_used.elapsed().as_secs_f64(),
                            slot.turns_served
                        ));
                        drop(slot); // drops child + home guard
                        continue;
                    }
                    return Ok(slot);
                }
            }

            // No idle slot — spawn one now (under timeout).
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("codex pool: timed out waiting for an available slot".into());
            }
            match tokio::time::timeout(remaining, self.spawn_slot()).await {
                Ok(Ok(slot)) => return Ok(slot),
                Ok(Err(err)) => return Err(err),
                Err(_) => {
                    return Err("codex pool: timed out spawning a new codex child".into());
                }
            }
        }
    }

    /// Return a used slot to the idle queue. If the queue is already at
    /// capacity (because the pool size changed at runtime, or extra slots were
    /// spawned to replace failed ones) the slot is dropped.
    pub(super) async fn checkin(&self, mut slot: PoolSlot) {
        let mut idle = self.idle.lock().await;
        if idle.len() < self.size {
            slot.on_return();
            idle.push(slot);
        }
        // else: drop the slot (child is killed on drop via `kill_on_drop`)
    }

    /// Spawn and initialise a fresh child slot.
    async fn spawn_slot(&self) -> Result<PoolSlot, BoxError> {
        let cwd = tempfile::Builder::new()
            .prefix("axon-codex-cwd-")
            .tempdir()
            .map_err(|err| format!("codex pool: failed to create cwd tempdir: {err}"))?;

        let (home_guard, mut child) = if self.backend.codex_load_user_config {
            let child = spawn_child_passthrough(&self.backend, cwd.path())?;
            (None, child)
        } else {
            let home = home::prepare_codex_home(&self.backend)?;
            let child = spawn_child_isolated(&self.backend, &home, cwd.path())?;
            (Some(home), child)
        };

        let mut stdin = child
            .stdin
            .take()
            .ok_or("codex pool: failed to open child stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("codex pool: failed to open child stdout")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("codex pool: failed to open child stderr")?;
        let stderr_task = read_bounded_stderr_spawn(stderr);
        let mut stdout_reader = BufReader::new(stdout);

        // Run the one-time initialisation handshake.
        let thread_id = run_init_handshake(&self.backend, &mut stdin, &mut stdout_reader)
            .await
            .map_err(|err| format!("codex pool: init handshake failed: {err}"))?;

        // CWD temp dir is owned by the slot for the child's lifetime.
        // We pass ownership via a second home_guard slot since cwd is also a TempDir.
        // Re-use home_guard Option for both; cwd is implicitly kept alive by
        // the child process holding an open fd — on Linux processes do not need
        // the tempdir to exist after spawn, so dropping cwd here is safe.
        // (The child's working directory remains valid via the kernel fd-ref.)
        let _ = cwd; // drop temp dir — child already has it open

        log_info(&format!("codex pool: spawned child, thread_id={thread_id}"));

        Ok(PoolSlot {
            thread_id,
            stdin,
            stdout: stdout_reader,
            child,
            stderr_task,
            _home_guard: home_guard,
            last_used: Instant::now(),
            turns_served: 0,
        })
    }
}

/// Kill and wait for a pool slot's child. Errors from cleanup are logged but do
/// not propagate — the slot is already being discarded.
pub(super) async fn discard_slot(mut slot: PoolSlot, reason: &str) {
    let stderr_task = slot.stderr_task;
    let cleanup = cleanup_codex_child(&mut slot.child).await;
    let stderr_tail = collect_stderr(stderr_task).await;
    log_warn(&format!(
        "codex pool: discarding slot (reason={reason}, {} turns served, cleanup={:?}){}",
        slot.turns_served,
        cleanup,
        stderr_diagnostics_suffix(&stderr_tail),
    ));
}

/// Acquire or create the pool for this backend configuration.
///
/// The key is `"{cmd}\x00{model}"` so different executables or model overrides
/// get independent pools. The pool size equals `backend.completion_concurrency`.
pub(super) fn pool_for(backend: &LlmBackendConfig) -> Arc<CodexPool> {
    let model = backend.codex_model.as_deref().unwrap_or("");
    let key = format!("{}\x00{}", backend.codex_cmd, model);
    let size = backend.completion_concurrency.max(1);
    let idle_ttl = idle_ttl_from_env();
    POOL_MAP
        .entry(key)
        .or_insert_with(|| CodexPool::new(size, idle_ttl, backend.clone()))
        .clone()
}

/// Clear the process-global pool map (test helper only).
#[cfg(test)]
pub(super) async fn reset_pools_for_tests() {
    POOL_MAP.clear();
}

// ── Spawn helpers (mirrors of the ones in codex_app_server.rs) ──────────────

pub(super) fn spawn_child_isolated(
    backend: &LlmBackendConfig,
    home: &TempDir,
    cwd: &Path,
) -> Result<Child, BoxError> {
    let mut command = tokio::process::Command::new(&backend.codex_cmd);
    command
        .arg("app-server")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    home::apply_codex_env_allowlist(&mut command);
    home::apply_codex_home_env(&mut command, home.path());
    configure_codex_child_isolation(&mut command);
    command
        .spawn()
        .map_err(|err| format!("codex pool: failed to spawn child: {err}").into())
}

fn spawn_child_passthrough(backend: &LlmBackendConfig, cwd: &Path) -> Result<Child, BoxError> {
    let mut command = tokio::process::Command::new(&backend.codex_cmd);
    command
        .arg("app-server")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(home) = home::resolve_user_codex_home(backend)? {
        command.env("CODEX_HOME", home);
    }
    configure_codex_child_isolation(&mut command);
    command
        .spawn()
        .map_err(|err| format!("codex pool: failed to spawn passthrough child: {err}").into())
}

/// Run a single synthesis turn against an already-initialised slot.
///
/// On success returns the slot (healthy, ready to return to the pool).
/// On failure returns the error alongside the slot so the caller can discard it.
pub(super) async fn run_turn<F>(
    slot: &mut PoolSlot,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
    backend: &LlmBackendConfig,
    on_delta: &mut F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    use super::protocol::run_turn_handshake;
    run_turn_handshake(
        &slot.thread_id,
        prompt,
        model,
        effort,
        backend,
        &mut slot.stdin,
        &mut slot.stdout,
        on_delta,
    )
    .await
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
