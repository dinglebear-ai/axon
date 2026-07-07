//! Real shell command execution for the palette's Terminal tool.
//!
//! This is a genuine local shell — the user types a command line, we spawn
//! it for real via `tokio::process::Command`, capture real stdout/stderr, and
//! return it to the frontend. There is no allowlist/sandbox by design: this is
//! the same trust level as a desktop IDE's integrated terminal (e.g. VS Code)
//! — the app already runs with the user's own privileges, and a terminal that
//! can only run some commands isn't a terminal. The one thing we own is the
//! session's working directory, since each spawned process gets a fresh `cwd`
//! that doesn't persist to the next spawn — `cd` is special-cased so the
//! session behaves like a real interactive shell.
//!
//! No command ever arrives except through direct user keystrokes in the
//! Terminal component (there is no network/IPC path into this module other
//! than the Tauri `invoke` call the frontend makes for its own textbox).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tokio::process::Command;

/// Session-scoped working directory, shared across `terminal_run` invocations
/// for the life of the app. A spawned shell can't mutate the parent process's
/// cwd, so `cd` is handled here instead of being passed through to the shell.
pub(crate) struct TerminalState {
    cwd: Mutex<PathBuf>,
}

impl TerminalState {
    pub(crate) fn new() -> Self {
        Self {
            cwd: Mutex::new(initial_cwd()),
        }
    }
}

fn initial_cwd() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerminalRunResult {
    stdout: String,
    stderr: String,
    /// Process exit code. `None` when the process was terminated by a signal
    /// (Unix) rather than exiting normally.
    exit_code: Option<i32>,
    /// Working directory *after* the command ran — the frontend uses this to
    /// keep its prompt in sync, especially after a `cd`.
    cwd: String,
}

/// Resolve the user's shell: `$SHELL` first, then a platform-appropriate
/// fallback. Unix-first — Windows support is a documented follow-up.
fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else if Path::new("/bin/bash").exists() {
                "/bin/bash".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

/// Shell metacharacters that mean the command line is NOT a bare `cd` — e.g.
/// `cd /tmp && pwd`, `cd foo; ls`, `cd "$(bar)"`. Anything containing one of
/// these must fall through to the real spawned-shell path so it gets real
/// shell parsing instead of being misread as a literal `cd` target.
const SHELL_METACHARACTERS: [char; 9] = ['&', '|', ';', '$', '`', '<', '>', '(', ')'];

/// Split a `cd` invocation off the front of a command line, if the whole
/// command line is exactly a `cd`. Only a bare `cd`/`cd <target>` is
/// special-cased — anything with shell metacharacters (`cd foo && ls`, `cd
/// foo; ls`, command substitution, redirection, subshells, etc.) is left
/// alone and runs for real in the spawned shell instead (its `cd` just won't
/// outlive that one process, matching real shell semantics for compound
/// commands run through `sh -c`).
fn parse_cd_target(command: &str) -> Option<&str> {
    let trimmed = command.trim();
    let rest = trimmed.strip_prefix("cd")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None; // e.g. "cdfoo" is not `cd`
    }
    let target = rest.trim();
    if target.contains(SHELL_METACHARACTERS) || target.contains(char::is_whitespace) {
        return None; // compound command or multiple args — not a bare `cd`
    }
    Some(target)
}

/// Resolve a `cd` target against the current session cwd, honoring `~` and
/// relative paths the same way a real shell would.
fn resolve_cd_target(current: &Path, target: &str) -> PathBuf {
    if target.is_empty() {
        return dirs::home_dir().unwrap_or_else(|| current.to_path_buf());
    }
    if target == "~" {
        return dirs::home_dir().unwrap_or_else(|| current.to_path_buf());
    }
    if let Some(rest) = target.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    let candidate = Path::new(target);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        current.join(candidate)
    }
}

fn run_cd(state: &TerminalState, target: &str) -> TerminalRunResult {
    let mut guard = state.cwd.lock().unwrap_or_else(|err| err.into_inner());
    let resolved = resolve_cd_target(&guard, target);
    let canonical = match std::fs::canonicalize(&resolved) {
        Ok(path) => path,
        Err(err) => {
            return TerminalRunResult {
                stdout: String::new(),
                stderr: format!("cd: {target}: {err}"),
                exit_code: Some(1),
                cwd: guard.display().to_string(),
            };
        }
    };
    if !canonical.is_dir() {
        return TerminalRunResult {
            stdout: String::new(),
            stderr: format!("cd: not a directory: {target}"),
            exit_code: Some(1),
            cwd: guard.display().to_string(),
        };
    }
    *guard = canonical.clone();
    TerminalRunResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: Some(0),
        cwd: canonical.display().to_string(),
    }
}

/// Run one command line in the session's persistent working directory.
///
/// `cd` (bare, or with a single target) updates the session's tracked cwd
/// in-process rather than spawning a shell — a spawned process cannot change
/// its parent's directory, so without this every `cd` would silently no-op on
/// the next command. Everything else spawns the user's real login shell via
/// `-c` with `current_dir` set to the tracked cwd, capturing real stdout and
/// stderr.
#[tauri::command]
pub(crate) async fn terminal_run(
    state: tauri::State<'_, TerminalState>,
    command: String,
) -> Result<TerminalRunResult, String> {
    if command.trim().is_empty() {
        let cwd = current_cwd(&state);
        return Ok(TerminalRunResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            cwd,
        });
    }
    if let Some(target) = parse_cd_target(&command) {
        return Ok(run_cd(&state, target));
    }

    let cwd = current_cwd(&state);
    let shell = login_shell();
    let output = Command::new(&shell)
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| format!("failed to spawn {shell}: {err}"))?;

    Ok(TerminalRunResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        cwd,
    })
}

fn current_cwd(state: &TerminalState) -> String {
    state
        .cwd
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .display()
        .to_string()
}

/// Report the session's current working directory without running a command
/// — used by the frontend to seed the prompt on mount.
#[tauri::command]
pub(crate) fn terminal_cwd(state: tauri::State<'_, TerminalState>) -> String {
    current_cwd(&state)
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
