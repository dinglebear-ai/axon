use super::{clean_claude_project_name, parse_claude_jsonl};

// --- clean_claude_project_name ---

#[test]
fn clean_name_no_hyphen_returns_as_is() {
    assert_eq!(clean_claude_project_name("myproject"), "myproject");
    assert_eq!(clean_claude_project_name("axon"), "axon");
}

#[test]
fn clean_name_non_special_last_segment_returned() {
    // "foo-bar": last="bar", not a known suffix, so returns "bar"
    assert_eq!(clean_claude_project_name("foo-bar"), "bar");
}

#[test]
fn clean_name_known_suffix_rust() {
    // last="rust" is a known suffix, prev="axon", so returns "axon-rust"
    assert_eq!(
        clean_claude_project_name("workspace-axon-rust"),
        "axon-rust"
    );
}

#[test]
fn clean_name_known_suffix_rs() {
    assert_eq!(
        clean_claude_project_name("home-jmagar-myapp-rs"),
        "myapp-rs"
    );
}

#[test]
fn clean_name_known_suffix_git() {
    assert_eq!(clean_claude_project_name("project-repo-git"), "repo-git");
}

#[test]
fn clean_name_known_suffix_main() {
    assert_eq!(
        clean_claude_project_name("org-service-main"),
        "service-main"
    );
}

#[test]
fn clean_name_leading_hyphen_stripped_before_split() {
    // trim_start_matches('-') strips leading hyphens before splitting
    assert_eq!(clean_claude_project_name("-home-jmagar-axon"), "axon");
}

// --- parse_claude_jsonl ---

#[test]
fn parse_valid_claude_jsonl_string_content() {
    let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"Hello?\"}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":\"Sure!\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(result.text.contains("### USER:"));
    assert!(result.text.contains("Hello?"));
    assert!(result.text.contains("### ASSISTANT:"));
    assert!(result.text.contains("Sure!"));
}

#[test]
fn parse_valid_claude_jsonl_array_content() {
    let jsonl = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"What is Rust?\"}]}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"A systems language.\"}]}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(result.text.contains("What is Rust?"));
    assert!(result.text.contains("A systems language."));
}

#[test]
fn parse_claude_jsonl_skips_unknown_type() {
    let jsonl = "{\"type\":\"system\",\"message\":{\"content\":\"Hidden\"}}\n\
                 {\"type\":\"user\",\"message\":{\"content\":\"Visible\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(!result.text.contains("Hidden"));
    assert!(result.text.contains("Visible"));
}

#[test]
fn parse_claude_jsonl_malformed_lines_no_panic() {
    let jsonl = "not valid json\n\
                 {\"broken\":\n\
                 {\"type\":\"user\",\"message\":{\"content\":\"Fine\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(result.text.contains("Fine"));
}

#[test]
fn parse_claude_jsonl_empty_input_returns_empty() {
    assert!(parse_claude_jsonl("").text.trim().is_empty());
}

#[test]
fn parse_claude_jsonl_whitespace_only_content_skipped() {
    let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"   \"}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":\"Real\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(!result.text.contains("### USER:"));
    assert!(result.text.contains("Real"));
}

#[test]
fn parse_claude_jsonl_missing_content_field_skipped() {
    let jsonl = "{\"type\":\"user\",\"message\":{}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":\"OK\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(!result.text.contains("### USER:"));
    assert!(result.text.contains("OK"));
}

#[test]
fn parse_claude_jsonl_turn_count_counts_user_messages() {
    let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"Q1\"}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":\"A1\"}}\n\
                 {\"type\":\"user\",\"message\":{\"content\":\"Q2\"}}\n\
                 {\"type\":\"assistant\",\"message\":{\"content\":\"A2\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(result.turn_count, 2);
}

#[test]
fn parse_claude_jsonl_model_extracted_from_assistant() {
    let jsonl = "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-sonnet-4-6\",\"content\":\"Hello\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(result.model.as_deref(), Some("claude-sonnet-4-6"));
}

#[test]
fn parse_claude_jsonl_tool_use_detected() {
    let jsonl = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Glob\",\"input\":{}},{\"type\":\"text\",\"text\":\"Done\"}]}}";
    let result = parse_claude_jsonl(jsonl);
    assert!(result.has_tool_use);
    assert!(result.tools_used.contains(&"Glob".to_string()));
}

#[test]
fn parse_claude_jsonl_tools_used_is_sorted_and_deduplicated() {
    let jsonl = "{\"type\":\"assistant\",\"message\":{\"content\":[\
        {\"type\":\"tool_use\",\"name\":\"Read\"},\
        {\"type\":\"tool_use\",\"name\":\"Glob\"},\
        {\"type\":\"tool_use\",\"name\":\"Read\"},\
        {\"type\":\"text\",\"text\":\"done\"}\
    ]}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(result.tools_used, vec!["Glob", "Read"]);
}

#[test]
fn parse_claude_jsonl_workspace_and_branch_extracted() {
    let jsonl = "{\"type\":\"user\",\"cwd\":\"/home/user/project\",\"gitBranch\":\"main\",\"message\":{\"content\":\"Hi\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(result.workspace_path.as_deref(), Some("/home/user/project"));
    assert_eq!(result.git_branch.as_deref(), Some("main"));
}

#[test]
fn parse_claude_jsonl_last_message_at_is_latest_timestamp() {
    let jsonl = "{\"type\":\"user\",\"timestamp\":\"2024-01-01T10:00:00Z\",\"message\":{\"content\":\"Hi\"}}\n\
                 {\"type\":\"assistant\",\"timestamp\":\"2024-01-01T10:00:05Z\",\"message\":{\"content\":\"Hello\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(
        result.last_message_at.as_deref(),
        Some("2024-01-01T10:00:05Z")
    );
}

#[test]
fn parse_claude_jsonl_meta_lines_skipped_for_turns() {
    let jsonl = "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":\"meta\"}}\n\
                 {\"type\":\"user\",\"message\":{\"content\":\"real\"}}";
    let result = parse_claude_jsonl(jsonl);
    assert_eq!(result.turn_count, 1);
    assert!(!result.text.contains("meta"));
    assert!(result.text.contains("real"));
}
