//! Terminal subprocess state for the ACP bridge.
//!
//! All types use `Rc<RefCell<...>>` (not `Arc<Mutex>`) because `AcpBridgeClient`
//! is `?Send` and runs on a `current_thread` runtime inside a `LocalSet`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::ExitStatus;
use std::rc::Rc;

use tokio::io::AsyncReadExt;
use tokio::process::Child;

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
    /// Whether the ring buffer has ever been trimmed (byte limit hit).
    pub truncated: bool,
    /// Exit status, set once the process has exited.
    pub exit_status: Option<ExitStatus>,
    /// Maximum bytes retained in the output ring buffer.
    pub byte_limit: usize,
}

// ── Thread-local terminal registry ────────────────────────────────────────────

thread_local! {
    static TERMINALS: RefCell<HashMap<TerminalId, TerminalState>> =
        RefCell::new(HashMap::new());
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

    /// Spawn a new terminal subprocess and register it in the thread-local registry.
    ///
    /// Validates that `cwd` is an existing directory, generates a UUID terminal ID,
    /// spawns the process with stdout/stderr piped and stdin null, and starts a
    /// `spawn_local` task that reads all output into a bounded ring buffer.
    #[allow(dead_code)]
    pub async fn create(
        cmd: &str,
        args: &[&str],
        cwd: &Path,
        byte_limit: usize,
    ) -> Result<TerminalId, String> {
        // Validate cwd is an existing directory.
        if !cwd.is_dir() {
            return Err(format!(
                "cwd is not an existing directory: {}",
                cwd.display()
            ));
        }

        // Generate unique terminal ID.
        let id = TerminalId(uuid::Uuid::new_v4().to_string());

        // Spawn the subprocess with piped stdout/stderr and null stdin.
        let mut child = tokio::process::Command::new(cmd)
            .args(args)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn '{cmd}': {e}"))?;

        // Take stdout/stderr handles before storing child.
        let mut stdout = child.stdout.take().expect("stdout was piped");
        let mut stderr = child.stderr.take().expect("stderr was piped");

        // Shared ring buffer.
        let buf: Rc<RefCell<VecDeque<u8>>> = Rc::new(RefCell::new(VecDeque::new()));
        let buf_stdout = Rc::clone(&buf);
        let buf_stderr = Rc::clone(&buf);

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
                        }
                    }
                }
            }
        });

        let state = TerminalState {
            child: Some(child),
            output_buf: buf,
            truncated: false,
            exit_status: None,
            byte_limit,
        };

        let id_clone = id.clone();
        TERMINALS.with(|t| {
            t.borrow_mut().insert(id_clone, state);
        });

        Ok(id)
    }

    /// Drain the output buffer for a terminal.
    ///
    /// Returns `(output_bytes, truncated)` where `truncated` indicates whether
    /// the ring buffer hit the byte limit at any point.
    #[allow(dead_code)]
    pub async fn output(id: &TerminalId) -> Result<Vec<u8>, String> {
        TERMINALS.with(|t| {
            let map = t.borrow();
            let state = map
                .get(id)
                .ok_or_else(|| format!("terminal not found: {id}"))?;
            let bytes: Vec<u8> = state.output_buf.borrow().iter().copied().collect();
            Ok(bytes)
        })
    }

    /// Wait for the terminal subprocess to exit and return its exit code.
    ///
    /// If the process has already exited, returns the cached exit code.
    /// Otherwise takes the `Child` handle, awaits `child.wait()`, stores the
    /// `ExitStatus`, and returns the code.
    #[allow(dead_code)]
    pub async fn wait_for_exit(id: &TerminalId) -> Result<i32, String> {
        // Check if already exited.
        let already_exited = TERMINALS.with(|t| {
            t.borrow()
                .get(id)
                .and_then(|s| s.exit_status)
                .map(|s| s.code().unwrap_or(-1))
        });
        if let Some(code) = already_exited {
            return Ok(code);
        }

        // Take child out to await without holding borrow.
        let child = TERMINALS.with(|t| t.borrow_mut().get_mut(id).and_then(|s| s.child.take()));

        let Some(mut child) = child else {
            return Err(format!("terminal has no child handle: {id}"));
        };

        let status = child
            .wait()
            .await
            .map_err(|e| format!("wait() failed: {e}"))?;

        // Store exit status back.
        TERMINALS.with(|t| {
            if let Some(state) = t.borrow_mut().get_mut(id) {
                state.exit_status = Some(status);
            }
        });

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
                let id =
                    TerminalManager::create("echo", &["hello"], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
                        .await
                        .expect("create should succeed");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let output = TerminalManager::output(&id)
                    .await
                    .expect("output should succeed");
                let exit_code = TerminalManager::wait_for_exit(&id)
                    .await
                    .expect("wait_for_exit should succeed");
                let output_str = String::from_utf8_lossy(&output);
                assert!(
                    output_str.contains("hello"),
                    "output did not contain 'hello': {output_str}"
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
}
