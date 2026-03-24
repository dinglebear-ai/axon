//! Terminal subprocess state for the ACP bridge.
//!
//! All types use `Rc<RefCell<...>>` (not `Arc<Mutex>`) because `AcpBridgeClient`
//! is `?Send` and runs on a `current_thread` runtime inside a `LocalSet`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::ExitStatus;
use std::rc::Rc;

use agent_client_protocol;
use tokio::io::AsyncReadExt;
use tokio::process::Child;

// ── TerminalError ─────────────────────────────────────────────────────────────

/// Typed error variants for `TerminalManager` operations.
#[derive(Debug)]
pub enum TerminalError {
    /// No terminal with the given ID exists in this manager.
    NotFound,
    /// The terminal process has already exited.
    AlreadyExited,
    /// Failed to spawn the requested subprocess.
    SpawnFailed(String),
    /// Failed to send a kill signal to the subprocess.
    KillFailed(String),
    /// The requested `cwd` path would escape the session working directory.
    CwdEscaped,
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "terminal not found"),
            Self::AlreadyExited => write!(f, "terminal has already exited"),
            Self::SpawnFailed(msg) => write!(f, "spawn failed: {msg}"),
            Self::KillFailed(msg) => write!(f, "kill failed: {msg}"),
            Self::CwdEscaped => write!(f, "cwd escaped session working directory"),
        }
    }
}

impl std::error::Error for TerminalError {}

impl From<TerminalError> for agent_client_protocol::Error {
    fn from(e: TerminalError) -> Self {
        match e {
            TerminalError::NotFound => {
                agent_client_protocol::Error::resource_not_found(Some(e.to_string()))
            }
            TerminalError::AlreadyExited
            | TerminalError::SpawnFailed(_)
            | TerminalError::KillFailed(_)
            | TerminalError::CwdEscaped => agent_client_protocol::Error::internal_error(),
        }
    }
}

// ── TerminalId ────────────────────────────────────────────────────────────────

/// Opaque identifier for a managed terminal subprocess.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalId(pub String);

impl From<String> for TerminalId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TerminalId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for TerminalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── TerminalState ─────────────────────────────────────────────────────────────

/// Runtime state of a single managed terminal subprocess.
#[allow(dead_code)]
pub struct TerminalState {
    /// The running child process, if still alive.
    pub child: Option<Child>,
    /// Ring buffer of unread output bytes — shared with the reader task.
    pub output_buf: Rc<RefCell<VecDeque<u8>>>,
    /// Shared truncation flag — set to `true` by the reader task when bytes
    /// are dropped from the front of the ring buffer.
    pub truncated_flag: Rc<RefCell<bool>>,
    /// Whether the ring buffer has ever been trimmed (byte limit hit).
    /// Mirrors `truncated_flag` after the first `output()` drain; kept for
    /// backward-compatibility with tests that check the field directly.
    pub truncated: bool,
    /// Exit status, set once the process has exited.
    pub exit_status: Option<ExitStatus>,
    /// Maximum bytes retained in the output ring buffer.
    pub byte_limit: usize,
}

// ── TerminalManager ───────────────────────────────────────────────────────────

/// Manages a set of terminal subprocesses for an ACP session.
///
/// Uses `Rc<RefCell<...>>` so it can be shared across `spawn_local` tasks on
/// the same `LocalSet` without requiring `Send`.
#[allow(dead_code)]
#[derive(Clone)]
pub struct TerminalManager {
    pub terminals: Rc<RefCell<HashMap<TerminalId, TerminalState>>>,
}

impl TerminalManager {
    /// Create a new, empty `TerminalManager`.
    pub fn new() -> Self {
        Self {
            terminals: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Spawn a new terminal subprocess and register it in this manager's registry.
    ///
    /// Validates that `cwd` is an existing directory, generates a UUID terminal ID,
    /// spawns the process with stdout/stderr piped and stdin null, and starts a
    /// `spawn_local` task that reads all output into a bounded ring buffer.
    #[allow(dead_code)]
    pub async fn create(
        &self,
        cmd: &str,
        args: &[&str],
        cwd: &Path,
        byte_limit: usize,
    ) -> Result<TerminalId, TerminalError> {
        // Validate cwd is an existing directory.
        if !cwd.is_dir() {
            return Err(TerminalError::CwdEscaped);
        }

        // Generate unique terminal ID.
        let id = TerminalId(uuid::Uuid::new_v4().to_string());

        tracing::info!(
            terminal_id = %id.0,
            cmd = %cmd,
            args = ?args,
            cwd = %cwd.display(),
            "terminal: creating subprocess"
        );

        // Spawn the subprocess with piped stdout/stderr and null stdin.
        let mut child = tokio::process::Command::new(cmd)
            .args(args)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| TerminalError::SpawnFailed(format!("failed to spawn '{cmd}': {e}")))?;

        // Take stdout/stderr handles before storing child.
        let mut stdout = child.stdout.take().expect("stdout was piped");
        let mut stderr = child.stderr.take().expect("stderr was piped");

        // Shared ring buffer and truncation flag.
        let buf: Rc<RefCell<VecDeque<u8>>> = Rc::new(RefCell::new(VecDeque::new()));
        let truncated_flag: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let buf_stdout = Rc::clone(&buf);
        let buf_stderr = Rc::clone(&buf);
        let trunc_stdout = Rc::clone(&truncated_flag);
        let trunc_stderr = Rc::clone(&truncated_flag);

        // Spawn stdout reader task.
        tokio::task::spawn_local(async move {
            let mut tmp = [0u8; 4096];
            loop {
                match stdout.read(&mut tmp).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut b = buf_stdout.borrow_mut();
                        for &byte in &tmp[..n] {
                            b.push_back(byte);
                        }
                        // Trim front to enforce byte limit.
                        while b.len() > byte_limit {
                            b.pop_front();
                            *trunc_stdout.borrow_mut() = true;
                        }
                    }
                }
            }
        });

        // Spawn stderr reader task.
        tokio::task::spawn_local(async move {
            let mut tmp = [0u8; 4096];
            loop {
                match stderr.read(&mut tmp).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut b = buf_stderr.borrow_mut();
                        for &byte in &tmp[..n] {
                            b.push_back(byte);
                        }
                        while b.len() > byte_limit {
                            b.pop_front();
                            *trunc_stderr.borrow_mut() = true;
                        }
                    }
                }
            }
        });

        let state = TerminalState {
            child: Some(child),
            output_buf: buf,
            truncated: false,
            truncated_flag,
            exit_status: None,
            byte_limit,
        };

        self.terminals.borrow_mut().insert(id.clone(), state);

        tracing::info!(terminal_id = %id.0, "terminal: subprocess registered");

        Ok(id)
    }

    /// Drain the output buffer for a terminal.
    ///
    /// Returns `(output_text, truncated, exit_code)` where:
    /// - `output_text` is the UTF-8 (lossy) string from all buffered bytes
    /// - `truncated` is `true` if the ring buffer hit the byte limit at any point
    /// - `exit_code` is `Some(code)` if the process has exited, `None` if still running
    #[allow(dead_code)]
    pub fn output(&self, id: &TerminalId) -> Result<(String, bool, Option<i32>), TerminalError> {
        let mut map = self.terminals.borrow_mut();
        let state = map.get_mut(id).ok_or(TerminalError::NotFound)?;

        // Drain the ring buffer into a Vec<u8>.
        let bytes: Vec<u8> = state.output_buf.borrow_mut().drain(..).collect();

        // Read + reset the shared truncation flag.
        let was_truncated = *state.truncated_flag.borrow();
        state.truncated = was_truncated;

        let text = String::from_utf8_lossy(&bytes).into_owned();
        let exit_code = state.exit_status.and_then(|s| s.code());

        tracing::info!(
            terminal_id = %id.0,
            drained_bytes = bytes.len(),
            truncated = was_truncated,
            "terminal: output drained"
        );

        Ok((text, was_truncated, exit_code))
    }

    /// Send a kill signal to the terminal subprocess.
    ///
    /// If the process has already exited (or the terminal doesn't exist),
    /// this is a no-op and returns `Ok(())`.
    /// Otherwise, calls `start_kill()` on the child process (sends SIGKILL on Unix).
    /// The child handle is kept so `wait_for_exit` can still collect the exit status.
    #[allow(dead_code)]
    pub async fn kill(&self, id: &TerminalId) -> Result<(), TerminalError> {
        // If already exited or not found, nothing to do.
        let already_done = {
            let map = self.terminals.borrow();
            match map.get(id) {
                None => return Ok(()),
                Some(s) => s.exit_status.is_some(),
            }
        };
        if already_done {
            return Ok(());
        }

        // Take child out before await (cannot hold RefCell borrow across await).
        let child = self
            .terminals
            .borrow_mut()
            .get_mut(id)
            .and_then(|s| s.child.take());

        let Some(mut child) = child else {
            // Child already taken (e.g. by wait_for_exit) — nothing to kill.
            return Ok(());
        };

        // Send kill signal (SIGKILL on Unix via tokio).
        // Ignore errors: process may have exited between the check and now.
        tracing::info!(terminal_id = %id.0, "terminal: sending kill signal");
        let _ = child.start_kill();

        // Put the child back so wait_for_exit can collect the exit status.
        if let Some(state) = self.terminals.borrow_mut().get_mut(id) {
            state.child = Some(child);
        }

        Ok(())
    }

    /// Remove a terminal from the manager, killing it first if still running.
    ///
    /// Idempotent: if the terminal ID is not in the map (already released or
    /// never created), returns `Ok(())` without error (NFR-007).
    #[allow(dead_code)]
    pub async fn release(&self, id: &TerminalId) -> Result<(), TerminalError> {
        // If not in map, nothing to do (idempotent).
        let exists = self.terminals.borrow().contains_key(id);
        if !exists {
            return Ok(());
        }

        // Kill if still running (exit_status == None means process may be alive).
        let still_running = self
            .terminals
            .borrow()
            .get(id)
            .map(|s| s.exit_status.is_none())
            .unwrap_or(false);

        if still_running {
            self.kill(id).await?;
        }

        // Remove from map.
        self.terminals.borrow_mut().remove(id);

        tracing::info!(terminal_id = %id.0, "terminal: released and removed from registry");

        Ok(())
    }

    /// Wait for the terminal subprocess to exit and return its exit code.
    ///
    /// If the process has already exited, returns the cached exit code.
    /// Otherwise takes the `Child` handle, awaits `child.wait()`, stores the
    /// `ExitStatus`, and returns the code.
    #[allow(dead_code)]
    pub async fn wait_for_exit(&self, id: &TerminalId) -> Result<i32, TerminalError> {
        // Check if already exited.
        let already_exited = self
            .terminals
            .borrow()
            .get(id)
            .and_then(|s| s.exit_status)
            .map(|s| s.code().unwrap_or(-1));
        if let Some(code) = already_exited {
            return Ok(code);
        }

        // Take child out to await without holding borrow.
        let child = self
            .terminals
            .borrow_mut()
            .get_mut(id)
            .and_then(|s| s.child.take());

        let Some(mut child) = child else {
            return Err(TerminalError::AlreadyExited);
        };

        let status = child
            .wait()
            .await
            .map_err(|e| TerminalError::KillFailed(format!("wait() failed: {e}")))?;

        // Store exit status back.
        if let Some(state) = self.terminals.borrow_mut().get_mut(id) {
            state.exit_status = Some(status);
        }

        Ok(status.code().unwrap_or(-1))
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Default output byte limit for terminal buffers.
#[allow(dead_code)]
pub const DEFAULT_OUTPUT_BYTE_LIMIT: usize = 256 * 1024;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_create_terminal_output_wait() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mgr = TerminalManager::new();
                let id = mgr
                    .create("echo", &["hello"], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                    .await
                    .expect("create should succeed");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let exit_code = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("wait_for_exit should succeed");
                let (output_str, truncated, exit_code_from_output) =
                    mgr.output(&id).expect("output should succeed");
                assert!(
                    output_str.contains("hello"),
                    "output did not contain 'hello': {output_str}"
                );
                assert!(!truncated, "small output should not be truncated");
                assert_eq!(
                    exit_code_from_output,
                    Some(0),
                    "expected exit code Some(0) from output()"
                );
                assert_eq!(exit_code, 0, "expected exit code 0");
            })
            .await;
    }

    #[test]
    fn terminal_id_from_string() {
        let id = TerminalId::from("abc".to_string());
        assert_eq!(id.0, "abc");
    }

    #[test]
    fn terminal_id_from_str() {
        let id = TerminalId::from("xyz");
        assert_eq!(id.0, "xyz");
    }

    #[test]
    fn terminal_id_display() {
        let id = TerminalId::from("t-1");
        assert_eq!(id.to_string(), "t-1");
    }

    #[test]
    fn terminal_id_equality() {
        assert_eq!(TerminalId::from("a"), TerminalId::from("a"));
        assert_ne!(TerminalId::from("a"), TerminalId::from("b"));
    }

    #[test]
    fn terminal_manager_new_is_empty() {
        let mgr = TerminalManager::new();
        assert!(mgr.terminals.borrow().is_empty());
    }

    #[test]
    fn terminal_manager_default_is_empty() {
        let mgr = TerminalManager::default();
        assert!(mgr.terminals.borrow().is_empty());
    }

    /// RED: kill + release — not yet implemented.
    /// `manager.kill(&id)` and `manager.release(&id)` do not exist yet.
    /// This test must fail to compile until tasks 1.7 and 1.8 add those methods.
    #[tokio::test]
    async fn test_create_kill_release() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mgr = TerminalManager::new();
                let id = mgr
                    .create("sleep", &["60"], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                    .await
                    .expect("create should succeed");
                mgr.kill(&id).await.expect("kill should succeed");
                let _exit_code = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("should have exit after kill");
                mgr.release(&id).await.expect("release should succeed");
                assert!(
                    !mgr.terminals.borrow().contains_key(&id),
                    "terminal should be removed from map after release"
                );
            })
            .await;
    }

    /// Task 3.1: output buffer truncation — large output, small byte limit.
    /// Verifies `truncated = true` and that the returned bytes are the MOST
    /// RECENT bytes (tail of the stream), not the oldest.
    #[tokio::test]
    async fn test_output_truncation_returns_recent_bytes() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                // Use a 100-byte limit so a 1000-byte output definitely overflows.
                let byte_limit: usize = 100;
                // Produce exactly 1000 'x' characters followed by a newline.
                // The tail (most recent) should be 100 x's.
                let mgr = TerminalManager::new();
                let id = mgr
                    .create(
                        "python3",
                        &[
                            "-c",
                            "import sys; sys.stdout.write('x' * 1000); sys.stdout.flush()",
                        ],
                        &cwd,
                        byte_limit,
                    )
                    .await
                    .expect("create should succeed");

                // Wait for the command to finish and reader task to drain.
                let exit_code = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("wait_for_exit should succeed");
                assert_eq!(exit_code, 0, "python3 should exit 0");

                // Give the reader task a moment to flush remaining bytes.
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                let (output_str, truncated, _) = mgr.output(&id).expect("output should succeed");

                assert!(
                    truncated,
                    "output exceeding byte_limit must set truncated=true"
                );
                assert!(
                    output_str.len() <= byte_limit,
                    "returned bytes ({}) must not exceed byte_limit ({})",
                    output_str.len(),
                    byte_limit
                );
                // The ring buffer keeps the MOST RECENT bytes — all should be 'x'.
                assert!(
                    output_str.chars().all(|c| c == 'x'),
                    "retained bytes should be the most recent 'x' chars, got: {output_str:?}"
                );
            })
            .await;
    }

    /// Task 3.2: `wait_for_exit` on an already-exited terminal returns immediately.
    #[tokio::test]
    async fn test_wait_for_exit_already_exited() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mgr = TerminalManager::new();
                // `true` exits immediately with code 0.
                let id = mgr
                    .create("true", &[], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                    .await
                    .expect("create should succeed");

                // First wait — collects the real exit status.
                let code1 = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("first wait_for_exit should succeed");
                assert_eq!(code1, 0, "true exits with code 0");

                // Brief pause so the process is definitely gone.
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                // Second wait on an already-exited terminal — must return immediately
                // using the cached exit_status without blocking.
                let code2 = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("second wait_for_exit on already-exited terminal should succeed");
                assert_eq!(code2, 0, "cached exit code should still be 0");
            })
            .await;
    }

    /// Task 3.3: `kill` on an already-exited terminal is a no-op (returns Ok).
    #[tokio::test]
    async fn test_kill_already_exited_is_noop() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mgr = TerminalManager::new();
                // `true` exits immediately with code 0.
                let id = mgr
                    .create("true", &[], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                    .await
                    .expect("create should succeed");

                // Wait for process to exit and store exit_status.
                let exit_code = mgr
                    .wait_for_exit(&id)
                    .await
                    .expect("wait_for_exit should succeed");
                assert_eq!(exit_code, 0, "true exits with code 0");

                // Brief pause so the process is definitely gone.
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                // kill on an already-exited terminal must be Ok (no-op).
                mgr.kill(&id)
                    .await
                    .expect("kill on already-exited terminal should be a no-op Ok(())");
            })
            .await;
    }

    /// RED: double-release is a no-op — not yet implemented.
    /// `manager.kill(&id)` and `manager.release(&id)` do not exist yet.
    /// This test must fail to compile until tasks 1.7 and 1.8 add those methods.
    #[tokio::test]
    async fn test_double_release_noop() {
        let cwd = PathBuf::from("/tmp");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mgr = TerminalManager::new();
                let id = mgr
                    .create("sleep", &["60"], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                    .await
                    .expect("create should succeed");
                mgr.kill(&id).await.expect("kill failed");
                mgr.release(&id)
                    .await
                    .expect("first release should succeed");
                assert!(
                    !mgr.terminals.borrow().contains_key(&id),
                    "terminal should be removed after release"
                );
                mgr.release(&id)
                    .await
                    .expect("second release should be a no-op");
            })
            .await;
    }
}
