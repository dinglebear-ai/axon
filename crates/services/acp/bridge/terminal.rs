//! Terminal subprocess state for the ACP bridge.
//!
//! All types use `Rc<RefCell<...>>` (not `Arc<Mutex>`) because `AcpBridgeClient`
//! is `?Send` and runs on a `current_thread` runtime inside a `LocalSet`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::process::ExitStatus;
use std::rc::Rc;

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
    /// Ring buffer of unread output bytes.
    pub output_buf: VecDeque<u8>,
    /// Exit status, set once the process has exited.
    pub exit_status: Option<ExitStatus>,
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
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
