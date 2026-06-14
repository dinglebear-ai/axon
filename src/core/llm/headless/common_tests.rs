use super::*;

#[test]
fn headless_safety_rejects_forbidden_flags() {
    let spec = HeadlessCommandSpec {
        agent: "codex",
        program: "codex".to_string(),
        args: vec!["exec".to_string(), "--full-auto".to_string()],
        prompt_transport: PromptTransport::Stdin,
        output_mode: "jsonl",
    };
    assert!(spec.validate().is_err());
}

#[test]
fn headless_safety_redacts_and_bounds_stderr() {
    let raw = format!(
        "{} OPENAI_API_KEY=sk-secret TOKEN=atk_token normal",
        "x".repeat(STDERR_TAIL_LIMIT + 64)
    );
    let redacted = redacted_stderr_tail(raw.as_bytes());
    assert!(redacted.len() <= STDERR_TAIL_LIMIT + 128);
    assert!(!redacted.contains("sk-secret"));
    assert!(!redacted.contains("atk_token"));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn headless_safety_redacts_compact_json_secrets() {
    let redacted = redacted_stderr_tail(
        br#"{"error":"bad auth","api_key":"sk-secret","nested":{"token":"atk_token"}}"#,
    );

    assert!(redacted.contains("bad auth"));
    assert!(redacted.contains("[REDACTED]"));
    assert!(!redacted.contains("sk-secret"));
    assert!(!redacted.contains("atk_token"));
}

#[test]
fn headless_safety_redacts_authorization_header_without_space_after_colon() {
    let redacted =
        redacted_stderr_tail(b"request failed Authorization:Bearer sk-secret-value normal");

    assert!(redacted.contains("[REDACTED]"));
    assert!(redacted.contains("normal"));
    assert!(!redacted.contains("Authorization:Bearer"));
    assert!(!redacted.contains("sk-secret-value"));
}

#[test]
fn headless_safety_keeps_only_stderr_tail() {
    let mut buf = Vec::new();
    append_bounded_tail(&mut buf, &vec![b'a'; STDERR_TAIL_LIMIT]);
    append_bounded_tail(&mut buf, b"tail");
    assert_eq!(buf.len(), STDERR_TAIL_LIMIT);
    assert!(buf.ends_with(b"tail"));
}

#[test]
fn joined_prompt_prepends_system() {
    assert_eq!(joined_prompt(Some("sys"), "user"), "sys\n\nuser");
    assert_eq!(joined_prompt(Some("  "), "user"), "user");
    assert_eq!(joined_prompt(None, "user"), "user");
}
