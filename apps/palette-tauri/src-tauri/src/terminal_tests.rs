use super::*;

/// Real end-to-end proof: `terminal_run`'s inner logic actually spawns a real
/// shell and captures real stdout. We call the same code path `terminal_run`
/// uses (bypassing the `tauri::State` wrapper, which needs a running app) via
/// a small helper that mirrors the command body.
async fn run_direct(state: &TerminalState, command: &str) -> TerminalRunResult {
    if command.trim().is_empty() {
        return TerminalRunResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            cwd: current_cwd(state),
        };
    }
    if let Some(target) = parse_cd_target(command) {
        return run_cd(state, target);
    }
    let cwd = current_cwd(state);
    let shell = login_shell();
    let output = Command::new(&shell)
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .kill_on_drop(true)
        .output()
        .await
        .expect("spawn shell");
    TerminalRunResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        cwd,
    }
}

#[tokio::test]
async fn echo_runs_a_real_process_and_captures_real_stdout() {
    let state = TerminalState::new();
    let result = run_direct(&state, "echo hello-from-real-shell").await;
    assert_eq!(result.stdout.trim(), "hello-from-real-shell");
    assert_eq!(result.stderr, "");
    assert_eq!(result.exit_code, Some(0));
}

#[tokio::test]
async fn pwd_reports_the_tracked_session_cwd() {
    let state = TerminalState::new();
    let expected = current_cwd(&state);
    let result = run_direct(&state, "pwd").await;
    // Canonicalize both sides — /tmp can be a symlink to /private/tmp etc.
    let actual = std::fs::canonicalize(result.stdout.trim()).unwrap();
    let expected_canon = std::fs::canonicalize(&expected).unwrap();
    assert_eq!(actual, expected_canon);
}

#[tokio::test]
async fn nonzero_exit_code_is_captured() {
    let state = TerminalState::new();
    let result = run_direct(&state, "exit 7").await;
    assert_eq!(result.exit_code, Some(7));
}

#[tokio::test]
async fn stderr_is_captured_separately_from_stdout() {
    let state = TerminalState::new();
    let result = run_direct(&state, "echo out-line; echo err-line 1>&2").await;
    assert_eq!(result.stdout.trim(), "out-line");
    assert_eq!(result.stderr.trim(), "err-line");
}

#[tokio::test]
async fn cd_persists_across_subsequent_commands_in_the_session() {
    let state = TerminalState::new();
    let tmp = std::env::temp_dir();
    let tmp_canon = std::fs::canonicalize(&tmp).unwrap();

    let cd_result = run_direct(&state, &format!("cd {}", tmp.display())).await;
    assert_eq!(cd_result.exit_code, Some(0));
    assert_eq!(std::path::Path::new(&cd_result.cwd), tmp_canon);

    // A brand-new spawned process for the *next* command should still see the
    // updated cwd — proving the session state (not the shell process) is what
    // carries the directory forward.
    let pwd_result = run_direct(&state, "pwd").await;
    let actual = std::fs::canonicalize(pwd_result.stdout.trim()).unwrap();
    assert_eq!(actual, tmp_canon);
}

#[tokio::test]
async fn cd_to_nonexistent_directory_reports_an_error_and_keeps_old_cwd() {
    let state = TerminalState::new();
    let before = current_cwd(&state);
    let result = run_direct(&state, "cd /definitely/does/not/exist/anywhere").await;
    assert_eq!(result.exit_code, Some(1));
    assert!(!result.stderr.is_empty());
    assert_eq!(result.cwd, before);
}

#[tokio::test]
async fn cd_with_no_target_goes_home() {
    let state = TerminalState::new();
    let result = run_direct(&state, "cd").await;
    assert_eq!(result.exit_code, Some(0));
    if let Some(home) = dirs::home_dir() {
        let home_canon = std::fs::canonicalize(&home).unwrap();
        assert_eq!(std::path::Path::new(&result.cwd), home_canon);
    }
}

#[tokio::test]
async fn compound_command_with_cd_does_not_leak_into_session_cwd() {
    // `cd foo && ls` is not the bare-`cd` fast path, so it spawns a real shell.
    // That subshell's cd cannot outlive the process — matching real shell
    // semantics for compound commands run through `sh -c`.
    let state = TerminalState::new();
    let before = current_cwd(&state);
    let tmp = std::env::temp_dir();
    let result = run_direct(&state, &format!("cd {} && pwd", tmp.display())).await;
    assert_eq!(result.exit_code, Some(0));
    let printed = std::fs::canonicalize(result.stdout.trim()).unwrap();
    let tmp_canon = std::fs::canonicalize(&tmp).unwrap();
    assert_eq!(printed, tmp_canon);
    // Session cwd tracked by our state is untouched.
    assert_eq!(current_cwd(&state), before);
}

#[test]
fn parse_cd_target_recognizes_bare_cd_and_rejects_prefix_matches() {
    assert_eq!(parse_cd_target("cd"), Some(""));
    assert_eq!(parse_cd_target("cd /tmp"), Some("/tmp"));
    assert_eq!(parse_cd_target("  cd   /tmp  "), Some("/tmp"));
    assert_eq!(parse_cd_target("cd~"), None);
    assert_eq!(parse_cd_target("cdfoo"), None);
    assert_eq!(parse_cd_target("echo cd"), None);
}

#[test]
fn resolve_cd_target_handles_tilde_and_relative_paths() {
    let current = PathBuf::from("/home/user/project");
    if let Some(home) = dirs::home_dir() {
        assert_eq!(resolve_cd_target(&current, "~"), home);
        assert_eq!(resolve_cd_target(&current, "~/docs"), home.join("docs"));
    }
    assert_eq!(
        resolve_cd_target(&current, "sub"),
        PathBuf::from("/home/user/project/sub")
    );
    assert_eq!(
        resolve_cd_target(&current, "/absolute/path"),
        PathBuf::from("/absolute/path")
    );
}

#[test]
fn login_shell_prefers_shell_env_var() {
    // SAFETY: single-threaded test process env mutation, restored immediately.
    unsafe {
        std::env::set_var("SHELL", "/bin/zsh");
    }
    assert_eq!(login_shell(), "/bin/zsh");
    unsafe {
        std::env::remove_var("SHELL");
    }
}

#[test]
fn current_cwd_reflects_state_without_running_anything() {
    let state = TerminalState::new();
    assert_eq!(current_cwd(&state), initial_cwd().display().to_string());
}

#[test]
fn parse_cd_target_rejects_compound_commands_with_shell_metacharacters() {
    // Regression: `cd /tmp && pwd` was previously matched as a bare `cd`
    // whose target was the literal string "/tmp && pwd", which then failed to
    // canonicalize. Compound commands must fall through to the real shell.
    assert_eq!(parse_cd_target("cd /tmp && pwd"), None);
    assert_eq!(parse_cd_target("cd /tmp; ls"), None);
    assert_eq!(parse_cd_target("cd /tmp | cat"), None);
    assert_eq!(parse_cd_target("cd $(pwd)"), None);
    assert_eq!(parse_cd_target("cd `pwd`"), None);
    assert_eq!(parse_cd_target("cd foo bar"), None);
}
