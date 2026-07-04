use super::*;

#[test]
fn parses_claude_session_target() {
    let t = parse_session_target("session:claude:abc123").unwrap();
    assert_eq!(t.provider, "claude");
    assert_eq!(t.session_id, "abc123");
}

#[test]
fn parses_codex_session_target() {
    let t = parse_session_target("session:codex:def-456").unwrap();
    assert_eq!(t.provider, "codex");
    assert_eq!(t.session_id, "def-456");
}

#[test]
fn session_id_may_contain_additional_colons() {
    // split_once keeps everything after the first `:` in session_id, matching
    // the router's canonical_session() behavior.
    let t = parse_session_target("session:gemini:2026-06-30:chat-1").unwrap();
    assert_eq!(t.provider, "gemini");
    assert_eq!(t.session_id, "2026-06-30:chat-1");
}

#[test]
fn rejects_missing_scheme() {
    assert!(parse_session_target("claude:abc123").is_err());
    assert!(parse_session_target("https://example.com").is_err());
}

#[test]
fn rejects_missing_session_id() {
    let err = parse_session_target("session:claude").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.session.target.format");
}

#[test]
fn rejects_empty_provider() {
    let err = parse_session_target("session::abc123").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.session.target.provider");
}

#[test]
fn rejects_empty_session_id() {
    let err = parse_session_target("session:claude:").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.session.target.session_id");
}
