use super::parse_codex_jsonl;

// --- parse_codex_jsonl ---

#[test]
fn parse_valid_codex_jsonl_text_field() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"How do I use async/await?\"}]}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Use the async keyword.\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.text.contains("### USER:"));
    assert!(result.text.contains("How do I use async/await?"));
    assert!(result.text.contains("### ASSISTANT:"));
    assert!(result.text.contains("Use the async keyword."));
}

#[test]
fn parse_valid_codex_jsonl_input_text_field() {
    // input_text is the alternate key name for user input blocks
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"input_text\":\"Explain ownership\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.text.contains("Explain ownership"));
}

#[test]
fn parse_codex_jsonl_skips_non_response_item_types() {
    let jsonl = "{\"type\":\"session_start\",\"payload\":{\"id\":\"sess-abc\"}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Hello!\"}]}}\n\
                 {\"type\":\"session_end\",\"payload\":{}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(!result.text.contains("sess-abc"));
    assert!(result.text.contains("Hello!"));
}

#[test]
fn parse_codex_jsonl_malformed_lines_no_panic() {
    let jsonl = "this is not json\n\
                 {\"incomplete\":\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Valid\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.text.contains("Valid"));
}

#[test]
fn parse_codex_jsonl_empty_input_returns_empty() {
    assert!(parse_codex_jsonl("").text.trim().is_empty());
}

#[test]
fn parse_codex_jsonl_multiple_blocks_concatenated() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Part A. \"},{\"text\":\"Part B.\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.text.contains("Part A."));
    assert!(result.text.contains("Part B."));
}

#[test]
fn parse_codex_jsonl_whitespace_only_content_skipped() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"   \"}]}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Answer\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(!result.text.contains("### USER:"));
    assert!(result.text.contains("Answer"));
}

#[test]
fn parse_codex_jsonl_unknown_role_falls_back_to_unknown() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"content\":[{\"text\":\"Mystery\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.text.contains("### UNKNOWN:"));
    assert!(result.text.contains("Mystery"));
}

#[test]
fn parse_codex_jsonl_turn_count_counts_user_messages() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Q1\"}]}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"A1\"}]}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Q2\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert_eq!(result.turn_count, 2);
}

#[test]
fn parse_codex_jsonl_workspace_and_model_from_session_meta() {
    let jsonl = "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/user/proj\",\"model\":\"gpt-4o\"}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Hi\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    assert_eq!(result.workspace_path.as_deref(), Some("/home/user/proj"));
    assert_eq!(result.model.as_deref(), Some("gpt-4o"));
}

#[test]
fn parse_codex_jsonl_model_provider_fallback() {
    let jsonl = "{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\"}}\n\
                 {\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Hi\"}]}}";
    let result = parse_codex_jsonl(jsonl);
    // Falls back to model_provider when model field is absent.
    assert_eq!(result.model.as_deref(), Some("openai"));
}

#[test]
fn parse_codex_jsonl_tool_use_detected() {
    let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[\
        {\"type\":\"tool_call\",\"name\":\"bash\"},\
        {\"type\":\"text\",\"text\":\"Done\"}\
    ]}}";
    let result = parse_codex_jsonl(jsonl);
    assert!(result.has_tool_use);
    assert!(result.tools_used.contains(&"bash".to_string()));
}
