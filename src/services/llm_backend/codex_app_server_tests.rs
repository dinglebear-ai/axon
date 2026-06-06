use super::*;

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
