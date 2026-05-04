//! ACP request validation helpers.
//!
//! Extracted from `mapping.rs` to keep that module under the 500-line monolith
//! limit. All public functions are re-exported from the parent `mapping` module.

use std::error::Error;
use std::path::{Path, PathBuf};

use crate::crates::services::types::{AcpPromptTurnRequest, AcpSessionProbeRequest};

// ── Validation helpers ──────────────────────────────────────────────────────

pub fn validate_adapter_command(
    adapter: &crate::crates::services::types::AcpAdapterCommand,
) -> Result<(), Box<dyn Error>> {
    let program = adapter.program.trim();
    if program.is_empty() {
        return Err("ACP adapter command cannot be empty".into());
    }

    // If the program looks like a path (contains separator), verify it resolves
    // to a real executable file. Bare names (e.g. "claude") are resolved by
    // execvp via PATH.
    let path = Path::new(program);
    if is_path_style_program(program) {
        validate_path_style_adapter(path)?;
    }

    // Reject known shell interpreters by basename to prevent command injection.
    // The check is unconditional — bare names like "sh" or "bash" must be
    // blocked just as firmly as full paths like "/bin/sh".
    const BLOCKED_SHELLS: &[&str] = &[
        "sh",
        "bash",
        "zsh",
        "fish",
        "dash",
        "ksh",
        "csh",
        "tcsh",
        "cmd",
        "powershell",
        "pwsh",
    ];

    // Derive the basename from the program string (handles both bare names and paths).
    let basename = Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program);
    let basename_lower = basename.to_ascii_lowercase();
    let stem = basename_lower
        .strip_suffix(".exe")
        .unwrap_or(&basename_lower);
    if BLOCKED_SHELLS.contains(&stem) {
        return Err(
            format!("ACP adapter command must not be a shell interpreter: {basename}").into(),
        );
    }

    // For path-style programs also check the resolved canonical path to catch
    // symlinks like /tmp/safe_name -> /bin/bash.
    if is_path_style_program(program)
        && let Ok(canonical) = std::fs::canonicalize(path)
        && let Some(canon_name) = canonical.file_name().and_then(|n| n.to_str())
    {
        let lower = canon_name.to_ascii_lowercase();
        let canon_stem = lower.strip_suffix(".exe").unwrap_or(&lower);
        if BLOCKED_SHELLS.contains(&canon_stem) {
            return Err(format!(
                "ACP adapter command resolves to a shell interpreter: {canon_name}"
            )
            .into());
        }
    }

    Ok(())
}

fn is_path_style_program(program: &str) -> bool {
    program.contains('/') || program.contains('\\')
}

fn validate_path_style_adapter(path: &Path) -> Result<(), Box<dyn Error>> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("ACP adapter path does not exist: {} ({e})", path.display()))?;
    if !canonical.is_file() {
        return Err(format!(
            "ACP adapter path exists but is not a file: {}",
            canonical.display()
        )
        .into());
    }
    if !is_executable_file(&canonical)? {
        return Err(format!(
            "ACP adapter path exists but is not executable: {}",
            canonical.display()
        )
        .into());
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> Result<bool, Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok(mode & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(path.is_file())
}

pub fn validate_prompt_turn_request(req: &AcpPromptTurnRequest) -> Result<(), Box<dyn Error>> {
    if req.prompt.is_empty() {
        return Err("ACP prompt turn requires at least one prompt block".into());
    }
    if req
        .session_id
        .as_deref()
        .is_some_and(|session_id| session_id.trim().is_empty())
    {
        return Err("ACP session_id cannot be blank when provided".into());
    }
    Ok(())
}

pub fn validate_probe_request(req: &AcpSessionProbeRequest) -> Result<(), Box<dyn Error>> {
    if req
        .session_id
        .as_deref()
        .is_some_and(|session_id| session_id.trim().is_empty())
    {
        return Err("ACP session_id cannot be blank when provided".into());
    }
    Ok(())
}

pub fn validate_session_cwd(cwd: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if !cwd.is_absolute() {
        return Err("ACP session cwd must be an absolute path".into());
    }
    if !cwd.exists() {
        return Err(format!("ACP session cwd does not exist: {}", cwd.display()).into());
    }
    if !cwd.is_dir() {
        return Err(format!(
            "ACP session cwd exists but is not a directory: {}",
            cwd.display()
        )
        .into());
    }
    Ok(cwd.to_path_buf())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::AcpAdapterCommand;

    // ── validate_adapter_command: blocked shell names ────────────────────

    #[test]
    fn rejects_bare_sh() {
        let cmd = AcpAdapterCommand::new("sh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_bash() {
        let cmd = AcpAdapterCommand::new("bash", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_zsh() {
        let cmd = AcpAdapterCommand::new("zsh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_fish() {
        let cmd = AcpAdapterCommand::new("fish", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_dash() {
        let cmd = AcpAdapterCommand::new("dash", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_ksh() {
        let cmd = AcpAdapterCommand::new("ksh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_csh() {
        let cmd = AcpAdapterCommand::new("csh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_tcsh() {
        let cmd = AcpAdapterCommand::new("tcsh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_cmd() {
        let cmd = AcpAdapterCommand::new("cmd", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_powershell() {
        let cmd = AcpAdapterCommand::new("powershell", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_bare_pwsh() {
        let cmd = AcpAdapterCommand::new("pwsh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_adapter_command: case insensitivity ─────────────────────

    #[test]
    fn rejects_uppercase_bash() {
        let cmd = AcpAdapterCommand::new("BASH", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_mixed_case_bash() {
        let cmd = AcpAdapterCommand::new("Bash", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_uppercase_powershell() {
        let cmd = AcpAdapterCommand::new("POWERSHELL", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_adapter_command: .exe suffix ────────────────────────────

    #[test]
    fn rejects_bash_exe() {
        let cmd = AcpAdapterCommand::new("bash.exe", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_powershell_exe() {
        let cmd = AcpAdapterCommand::new("powershell.exe", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_cmd_exe() {
        let cmd = AcpAdapterCommand::new("cmd.exe", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_pwsh_exe() {
        let cmd = AcpAdapterCommand::new("pwsh.exe", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_uppercase_bash_exe() {
        let cmd = AcpAdapterCommand::new("BASH.EXE", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_adapter_command: full path to shell ─────────────────────

    #[test]
    fn rejects_bin_sh() {
        let cmd = AcpAdapterCommand::new("/bin/sh", vec![]);
        // /bin/sh exists on Linux — must be rejected via basename check.
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_usr_bin_bash() {
        let cmd = AcpAdapterCommand::new("/usr/bin/bash", vec![]);
        // Even if the path doesn't exist (some distros), basename "bash" is blocked.
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_usr_bin_zsh() {
        let cmd = AcpAdapterCommand::new("/usr/bin/zsh", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_adapter_command: legitimate adapters ────────────────────

    #[test]
    fn accepts_claude() {
        let cmd = AcpAdapterCommand::new("claude", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn accepts_codex() {
        let cmd = AcpAdapterCommand::new("codex", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn accepts_my_adapter() {
        let cmd = AcpAdapterCommand::new("my-adapter", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn accepts_custom_adapter() {
        let cmd = AcpAdapterCommand::new("custom_adapter", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn accepts_gemini() {
        let cmd = AcpAdapterCommand::new("gemini", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn accepts_bare_nonexistent_program_name() {
        let cmd = AcpAdapterCommand::new("definitely-not-installed-axon-test-adapter", vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[test]
    fn rejects_nonexistent_path_style_program() {
        let cmd = AcpAdapterCommand::new("/tmp/definitely-not-installed-axon-test-adapter", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_existing_path_style_directory() {
        let cmd = AcpAdapterCommand::new("/tmp", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn accepts_existing_executable_path_style_program() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let adapter = dir.path().join("adapter");
        std::fs::write(&adapter, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&adapter).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&adapter, perms).unwrap();

        let cmd = AcpAdapterCommand::new(adapter.to_string_lossy().to_string(), vec![]);
        assert!(validate_adapter_command(&cmd).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_non_executable_path_style_program() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let adapter = dir.path().join("adapter");
        std::fs::write(&adapter, "not executable\n").unwrap();
        let mut perms = std::fs::metadata(&adapter).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&adapter, perms).unwrap();

        let cmd = AcpAdapterCommand::new(adapter.to_string_lossy().to_string(), vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_adapter_command: empty program ─────────────────────────

    #[test]
    fn rejects_empty_program() {
        let cmd = AcpAdapterCommand::new("", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    #[test]
    fn rejects_whitespace_only_program() {
        let cmd = AcpAdapterCommand::new("   ", vec![]);
        assert!(validate_adapter_command(&cmd).is_err());
    }

    // ── validate_prompt_turn_request ─────────────────────────────────────

    #[test]
    fn rejects_empty_prompt() {
        let req = AcpPromptTurnRequest {
            session_id: None,
            prompt: vec![],
            model: None,
            session_mode: None,
            blocked_mcp_tools: vec![],
            mcp_servers: vec![],
        };
        assert!(validate_prompt_turn_request(&req).is_err());
    }

    #[test]
    fn rejects_blank_session_id() {
        let req = AcpPromptTurnRequest {
            session_id: Some("   ".to_string()),
            prompt: vec!["hello".to_string()],
            model: None,
            session_mode: None,
            blocked_mcp_tools: vec![],
            mcp_servers: vec![],
        };
        assert!(validate_prompt_turn_request(&req).is_err());
    }

    #[test]
    fn accepts_valid_prompt_turn() {
        let req = AcpPromptTurnRequest {
            session_id: Some("abc-123".to_string()),
            prompt: vec!["hello".to_string()],
            model: None,
            session_mode: None,
            blocked_mcp_tools: vec![],
            mcp_servers: vec![],
        };
        assert!(validate_prompt_turn_request(&req).is_ok());
    }

    #[test]
    fn accepts_prompt_turn_without_session_id() {
        let req = AcpPromptTurnRequest {
            session_id: None,
            prompt: vec!["hello".to_string()],
            model: None,
            session_mode: None,
            blocked_mcp_tools: vec![],
            mcp_servers: vec![],
        };
        assert!(validate_prompt_turn_request(&req).is_ok());
    }

    // ── validate_probe_request ───────────────────────────────────────────

    #[test]
    fn probe_rejects_blank_session_id() {
        let req = AcpSessionProbeRequest {
            session_id: Some("  ".to_string()),
            model: None,
        };
        assert!(validate_probe_request(&req).is_err());
    }

    #[test]
    fn probe_accepts_none_session_id() {
        let req = AcpSessionProbeRequest {
            session_id: None,
            model: None,
        };
        assert!(validate_probe_request(&req).is_ok());
    }

    #[test]
    fn probe_accepts_valid_session_id() {
        let req = AcpSessionProbeRequest {
            session_id: Some("session-42".to_string()),
            model: None,
        };
        assert!(validate_probe_request(&req).is_ok());
    }

    // ── validate_session_cwd ─────────────────────────────────────────────

    #[test]
    fn cwd_rejects_relative_path() {
        let result = validate_session_cwd(Path::new("relative/path"));
        assert!(result.is_err());
    }

    #[test]
    fn cwd_rejects_nonexistent_path() {
        let result = validate_session_cwd(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_err());
    }

    #[test]
    fn cwd_accepts_tmp() {
        // /tmp always exists on Linux.
        let result = validate_session_cwd(Path::new("/tmp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/tmp"));
    }

    #[test]
    fn cwd_rejects_file_not_dir() {
        // /etc/hostname exists on most Linux systems as a regular file.
        let path = Path::new("/etc/hostname");
        if path.is_file() {
            let result = validate_session_cwd(path);
            assert!(result.is_err());
        }
    }
}
