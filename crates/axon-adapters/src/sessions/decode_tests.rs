use super::*;

// --- Claude JSONL ---

#[test]
fn decode_claude_happy_path_extracts_turns_and_metadata() {
    let content = concat!(
        r#"{"type":"user","cwd":"/home/j/proj","gitBranch":"main","timestamp":"2026-01-01T00:00:00Z","message":{"content":"hello"}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-x","content":[{"type":"text","text":"hi there"},{"type":"tool_use","name":"Bash"}]}}"#,
    );
    let decoded = decode_claude_jsonl(content);
    assert!(decoded.text.contains("### USER:"));
    assert!(decoded.text.contains("hello"));
    assert!(decoded.text.contains("### ASSISTANT:"));
    assert!(decoded.text.contains("hi there"));
    assert_eq!(decoded.turn_count, 1);
    assert_eq!(decoded.model.as_deref(), Some("claude-x"));
    assert!(decoded.has_tool_use);
    assert_eq!(decoded.tools_used, vec!["Bash".to_string()]);
    assert_eq!(decoded.workspace_path.as_deref(), Some("/home/j/proj"));
    assert_eq!(decoded.git_branch.as_deref(), Some("main"));
    assert_eq!(
        decoded.last_message_at.as_deref(),
        Some("2026-01-01T00:00:01Z")
    );
    assert_eq!(decoded.malformed_lines, 0);
}

#[test]
fn decode_claude_skips_meta_lines() {
    let content = concat!(
        r#"{"type":"user","isMeta":true,"message":{"content":"meta noise"}}"#,
        "\n",
        r#"{"type":"user","message":{"content":"real turn"}}"#,
    );
    let decoded = decode_claude_jsonl(content);
    assert!(!decoded.text.contains("meta noise"));
    assert!(decoded.text.contains("real turn"));
    assert_eq!(decoded.turn_count, 1);
}

#[test]
fn decode_claude_degraded_file_counts_malformed_lines() {
    let content = "not json\nalso not json\n{ broken";
    let decoded = decode_claude_jsonl(content);
    assert!(decoded.text.is_empty());
    assert_eq!(decoded.malformed_lines, 3);
}

#[test]
fn decode_claude_empty_content_is_empty_session() {
    let decoded = decode_claude_jsonl("");
    assert_eq!(decoded, DecodedSession::default());
}

#[test]
fn decode_claude_redacts_secret_shaped_tokens() {
    // Build the secret-shaped token at runtime so no literal secret sits in
    // source (keeps secret scanners quiet while still exercising the redactor).
    let fake_secret = format!("sk-{}", "abcdef1234567890ABCDEF");
    let content = format!(r#"{{"type":"user","message":{{"content":"my key is {fake_secret}"}}}}"#);
    let decoded = decode_claude_jsonl(&content);
    assert!(!decoded.text.contains(&fake_secret));
    assert!(decoded.text.contains("[redacted-secret]"));
}

// --- Codex JSONL ---

#[test]
fn decode_codex_happy_path_extracts_turns_and_metadata() {
    let content = concat!(
        r#"{"type":"session_meta","payload":{"cwd":"/home/j/proj","model":"gpt-5-codex"}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"do the thing"}]}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"role":"assistant","content":[{"type":"function_call","name":"shell","text":"running"}]}}"#,
    );
    let decoded = decode_codex_jsonl(content);
    assert!(decoded.text.contains("### USER:"));
    assert!(decoded.text.contains("do the thing"));
    assert!(decoded.text.contains("### ASSISTANT:"));
    assert_eq!(decoded.turn_count, 1);
    assert_eq!(decoded.model.as_deref(), Some("gpt-5-codex"));
    assert!(decoded.has_tool_use);
    assert_eq!(decoded.tools_used, vec!["shell".to_string()]);
    assert_eq!(decoded.workspace_path.as_deref(), Some("/home/j/proj"));
    assert_eq!(decoded.malformed_lines, 0);
}

#[test]
fn decode_codex_ignores_non_response_lines() {
    let content = concat!(
        r#"{"type":"other_event","payload":{"role":"user","content":[{"type":"input_text","text":"ignored"}]}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"kept"}]}}"#,
    );
    let decoded = decode_codex_jsonl(content);
    assert!(!decoded.text.contains("ignored"));
    assert!(decoded.text.contains("kept"));
}

#[test]
fn decode_codex_degraded_file_counts_malformed_lines() {
    let content = "{not valid\nstill not valid";
    let decoded = decode_codex_jsonl(content);
    assert!(decoded.text.is_empty());
    assert_eq!(decoded.malformed_lines, 2);
}

#[test]
fn decode_codex_empty_content_is_empty_session() {
    let decoded = decode_codex_jsonl("");
    assert_eq!(decoded, DecodedSession::default());
}

// --- Gemini JSON ---

#[test]
fn decode_gemini_happy_path_extracts_turns() {
    let json = r#"{"messages":[{"type":"human","content":[{"text":"What is the capital of France?"}]},{"type":"model","content":[{"text":"Paris."}]}]}"#;
    let decoded = decode_gemini_json(json).expect("should parse");
    assert!(decoded.text.contains("### HUMAN:"));
    assert!(decoded.text.contains("What is the capital of France?"));
    assert!(decoded.text.contains("### MODEL:"));
    assert!(decoded.text.contains("Paris."));
    // Gemini decode does not track turn_count by role convention used elsewhere;
    // "human" is not "user", so turn_count stays 0 — matches append_turn's role check.
    assert_eq!(decoded.turn_count, 0);
}

#[test]
fn decode_gemini_multiple_text_items_concatenated() {
    let json =
        r#"{"messages":[{"type":"model","content":[{"text":"First. "},{"text":"Second."}]}]}"#;
    let decoded = decode_gemini_json(json).expect("should parse");
    assert!(decoded.text.contains("First."));
    assert!(decoded.text.contains("Second."));
}

#[test]
fn decode_gemini_malformed_json_returns_err() {
    assert!(decode_gemini_json("this is not json").is_err());
}

#[test]
fn decode_gemini_empty_messages_array_is_empty_session() {
    let decoded = decode_gemini_json(r#"{"messages":[]}"#).expect("should parse");
    assert!(decoded.text.trim().is_empty());
}

#[test]
fn decode_gemini_missing_messages_key_is_empty_session() {
    let decoded = decode_gemini_json(r#"{"conversations":[]}"#).expect("should parse");
    assert!(decoded.text.trim().is_empty());
}

#[test]
fn decode_gemini_whitespace_only_content_skipped() {
    let json = r#"{"messages":[{"type":"human","content":[{"text":"   "}]},{"type":"model","content":[{"text":"Real response"}]}]}"#;
    let decoded = decode_gemini_json(json).expect("should parse");
    assert!(!decoded.text.contains("### HUMAN:"));
    assert!(decoded.text.contains("Real response"));
}

#[test]
fn decode_gemini_missing_type_falls_back_to_unknown() {
    let json = r#"{"messages":[{"content":[{"text":"Mystery"}]}]}"#;
    let decoded = decode_gemini_json(json).expect("should parse");
    assert!(decoded.text.contains("### UNKNOWN:"));
}

// --- redact_session_text ---

#[test]
fn redact_leaves_normal_words_untouched() {
    assert_eq!(redact_session_text("hello world"), "hello world");
}

#[test]
fn redact_masks_known_secret_prefixes() {
    // Secret-shaped tokens assembled at runtime — no literal secret in source.
    let openai = format!("token=sk-{}", "abc123DEF456ghi789JKL");
    let github = format!("ghp_{}", "abcdefghijklmnopqrstuvwxyz012345");
    assert!(redact_session_text(&openai).contains("[redacted-secret]"));
    assert!(redact_session_text(&github).contains("[redacted-secret]"));
}
