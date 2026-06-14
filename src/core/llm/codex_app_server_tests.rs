use super::*;
use crate::core::llm::LlmBackendKind;
use std::time::Duration;

fn backend_with_cmd(cmd: &str) -> LlmBackendConfig {
    LlmBackendConfig {
        codex_cmd: cmd.to_string(),
        ..LlmBackendConfig::default()
    }
}

#[test]
fn joined_prompt_prepends_system() {
    assert_eq!(joined_prompt(Some("sys"), "user"), "sys\n\nuser");
    assert_eq!(joined_prompt(Some("  "), "user"), "user");
    assert_eq!(joined_prompt(None, "user"), "user");
}

#[test]
fn completion_timeout_is_at_least_one_second() {
    let zero = LlmBackendConfig {
        completion_timeout_secs: 0,
        ..LlmBackendConfig::default()
    };
    assert_eq!(zero.completion_timeout(), Duration::from_secs(1));
    let forty_five = LlmBackendConfig {
        completion_timeout_secs: 45,
        ..LlmBackendConfig::default()
    };
    assert_eq!(forty_five.completion_timeout(), Duration::from_secs(45));
}

#[test]
fn validate_codex_cmd_allows_bare_name() {
    assert!(validate_codex_cmd(&backend_with_cmd("codex")).is_ok());
}

#[test]
fn validate_codex_cmd_rejects_empty() {
    assert!(validate_codex_cmd(&backend_with_cmd("   ")).is_err());
}

#[test]
fn validate_codex_cmd_rejects_missing_path() {
    let err = validate_codex_cmd(&backend_with_cmd("/nonexistent/path/to/codex")).unwrap_err();
    assert!(err.to_string().contains("AXON_CODEX_CMD"));
}

#[test]
fn stderr_suffix_empty_for_blank() {
    assert_eq!(stderr_suffix(b""), "");
    assert_eq!(stderr_suffix(b"   \n  "), "");
}

#[test]
fn stderr_suffix_includes_nonblank_tail() {
    let suffix = stderr_suffix(b"something broke");
    assert!(suffix.contains("stderr:"));
    assert!(suffix.contains("something broke"));
}

#[cfg(unix)]
#[test]
fn validate_codex_cmd_rejects_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real-codex");
    std::fs::write(&target, "#!/bin/sh\n").unwrap();
    let link = dir.path().join("codex-link");
    symlink(&target, &link).unwrap();
    let err = validate_codex_cmd(&backend_with_cmd(link.to_str().unwrap())).unwrap_err();
    assert!(err.to_string().contains("symlink"), "got: {err}");
}

#[cfg(unix)]
#[test]
fn validate_codex_cmd_rejects_non_executable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-exec");
    std::fs::write(&file, "x").unwrap(); // default perms have no exec bit
    let err = validate_codex_cmd(&backend_with_cmd(file.to_str().unwrap())).unwrap_err();
    assert!(err.to_string().contains("not executable"), "got: {err}");
}

#[tokio::test]
async fn codex_completion_timeout_covers_slow_child_before_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow-codex");
    std::fs::write(&script, "#!/bin/sh\nsleep 5\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut req = CompletionRequest::new("hello");
    req.backend = LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_timeout_secs: 1,
        configured: true,
        ..LlmBackendConfig::default()
    };

    let started = std::time::Instant::now();
    let err = complete_text(req).await.unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(err.to_string().contains("timed out"));
}
