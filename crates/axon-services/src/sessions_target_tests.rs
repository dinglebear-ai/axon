use super::*;

#[test]
fn claude_jsonl_file_parses() {
    let sel = parse_session_selector("session:claude:/home/u/.claude/projects/abc123.jsonl")
        .expect("parses");
    assert_eq!(sel.provider, "claude");
    assert_eq!(sel.session_id, "abc123");
    // sessions_root is the file itself (adapter indexes exactly that export),
    // not the parent directory.
    assert_eq!(
        sel.sessions_root,
        PathBuf::from("/home/u/.claude/projects/abc123.jsonl")
    );
}

#[test]
fn gemini_json_file_parses() {
    let sel = parse_session_selector("session:gemini:/tmp/exports/sess-42.json").expect("parses");
    assert_eq!(sel.provider, "gemini");
    assert_eq!(sel.session_id, "sess-42");
    assert_eq!(
        sel.sessions_root,
        PathBuf::from("/tmp/exports/sess-42.json")
    );
}

#[test]
fn codex_provider_is_accepted() {
    let sel = parse_session_selector("session:codex:/data/run.jsonl").expect("parses");
    assert_eq!(sel.provider, "codex");
    assert_eq!(sel.session_id, "run");
    assert_eq!(sel.sessions_root, PathBuf::from("/data/run.jsonl"));
}

#[test]
fn provider_is_case_insensitive() {
    let sel = parse_session_selector("session:Claude:/data/run.jsonl").expect("parses");
    assert_eq!(sel.provider, "claude");
}

#[test]
fn relative_file_defaults_root_to_dot() {
    let sel = parse_session_selector("session:claude:run.jsonl").expect("parses");
    assert_eq!(sel.session_id, "run");
    assert_eq!(sel.sessions_root, PathBuf::from("."));
}

#[test]
fn is_session_selector_matches_valid_shapes() {
    assert!(is_session_selector("session:claude:/a/b.jsonl"));
    assert!(is_session_selector("session:gemini:/a/b.json"));
    // Leading/trailing whitespace is trimmed.
    assert!(is_session_selector("  session:codex:/a/b.jsonl  "));
}

#[test]
fn unknown_provider_is_rejected() {
    let err = parse_session_selector("session:cursor:/a/b.jsonl").unwrap_err();
    assert!(err.contains("unknown session provider"), "got: {err}");
    assert!(!is_session_selector("session:cursor:/a/b.jsonl"));
}

#[test]
fn missing_prefix_is_rejected() {
    assert!(parse_session_selector("/home/u/.claude/x.jsonl").is_err());
    assert!(!is_session_selector("/home/u/.claude/x.jsonl"));
    // A plain reddit/word/URL is not a session selector.
    assert!(!is_session_selector("r/rust"));
    assert!(!is_session_selector("https://example.com/guide"));
}

#[test]
fn missing_path_component_is_rejected() {
    // `session:claude` has no `:<path>`.
    assert!(parse_session_selector("session:claude").is_err());
    // Empty path after the provider colon.
    assert!(parse_session_selector("session:claude:").is_err());
    assert!(parse_session_selector("session:claude:   ").is_err());
}

#[test]
fn path_with_no_file_name_is_rejected() {
    // A bare root has no file stem, so there is no resolvable session id.
    assert!(parse_session_selector("session:claude:/").is_err());
}
