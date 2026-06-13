use super::*;

#[test]
fn gemini_headless_command_uses_yolo_for_skill_activation() {
    let spec = build_command(&HeadlessCommandRequest::new(
        None,
        Some("system".to_string()),
    ))
    .unwrap();
    let joined = spec.args.join(" ");
    assert_eq!(spec.prompt_transport, PromptTransport::Argument);
    assert!(joined.contains("--approval-mode yolo"));
    assert!(spec.args.windows(2).any(|w| w == ["--extensions", ""]));
    assert!(joined.contains("--model gemini-3.1-flash-lite-preview"));
}

#[test]
fn gemini_headless_command_honors_model_override() {
    let spec = build_command(&HeadlessCommandRequest::new(
        Some("gemini-3.1-pro-preview".to_string()),
        None,
    ))
    .unwrap();
    assert!(
        spec.args
            .windows(2)
            .any(|w| w == ["--model", "gemini-3.1-pro-preview"])
    );
}

#[test]
fn gemini_headless_uses_stdin_for_large_prompts() {
    let spec = build_command(&HeadlessCommandRequest::new(None, None)).unwrap();
    assert_eq!(
        effective_prompt_transport(&spec, "small prompt"),
        PromptTransport::Argument
    );
    let large_prompt = "x".repeat(PROMPT_ARG_MAX_BYTES + 1);
    assert_eq!(
        effective_prompt_transport(&spec, &large_prompt),
        PromptTransport::Stdin
    );
}

#[test]
fn gemini_headless_parser_streams_message_content() {
    let mut state = GeminiStreamState::default();
    let mut out = String::new();
    state
        .handle_line(
            r#"{"type":"message","role":"assistant","content":"hel","delta":true}"#,
            &mut |d| {
                out.push_str(d);
                Ok(())
            },
        )
        .unwrap();
    state
        .handle_line(
            r#"{"type":"message","role":"assistant","content":"lo","delta":true}"#,
            &mut |d| {
                out.push_str(d);
                Ok(())
            },
        )
        .unwrap();
    state
        .handle_line(
            r#"{"type":"result","status":"success","stats":{"tool_calls":0}}"#,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(out, "hello");
    assert_eq!(state.finish().unwrap(), "hello");
}

#[test]
fn gemini_headless_parser_rejects_non_skill_tool_events() {
    let mut state = GeminiStreamState::default();
    let err = state
        .handle_line(r#"{"type":"tool_use","name":"shell"}"#, &mut |_| Ok(()))
        .expect_err("non-skill tool calls must fail closed");
    assert!(err.to_string().contains("unexpected tool call"));
}

#[test]
fn gemini_headless_parser_allows_activate_skill_tool_event() {
    let mut state = GeminiStreamState::default();
    // activate_skill via old "name" field
    state
        .handle_line(
            r#"{"type":"tool_use","name":"activate_skill","input":{"name":"axon-rag-synthesize"}}"#,
            &mut |_| Ok(()),
        )
        .expect("activate_skill tool_use must be allowed");
}

#[test]
fn gemini_headless_parser_allows_activate_skill_tool_name_field() {
    let mut state = GeminiStreamState::default();
    // activate_skill via new "tool_name" field (Gemini CLI 0.41.2+)
    state
        .handle_line(
            r#"{"type":"tool_use","tool_name":"activate_skill","tool_id":"x","parameters":{}}"#,
            &mut |_| Ok(()),
        )
        .expect("activate_skill via tool_name field must be allowed");
}

#[test]
fn gemini_headless_parser_allows_update_topic_tool_event() {
    let mut state = GeminiStreamState::default();
    // update_topic is a Gemini 0.41.2+ internal session management tool — always harmless
    state
        .handle_line(
            r#"{"type":"tool_use","tool_name":"update_topic","tool_id":"update_topic_1_0","parameters":{"title":"Test"}}"#,
            &mut |_| Ok(()),
        )
        .expect("update_topic tool_use must be allowed");
}

#[test]
fn gemini_headless_parser_allows_any_tool_count_in_result() {
    let mut state = GeminiStreamState::default();
    // Stats tool_calls count is no longer gated — update_topic adds calls automatically.
    // Multiple tool calls are fine as long as per-event whitelist passes.
    state
        .handle_line(
            r#"{"type":"result","status":"success","stats":{"tool_calls":5}}"#,
            &mut |_| Ok(()),
        )
        .expect("high tool_calls count in result stats must be allowed");
    assert!(state.saw_success);
}

#[test]
fn gemini_headless_assembles_chunked_stdout() {
    let out = assemble_utf8_chunks(&[b"hello ", b"world"]).unwrap();
    assert_eq!(out, "hello world");
}

#[test]
fn gemini_headless_assembles_split_multibyte_codepoint() {
    let snowman = "hi \u{2603}".as_bytes();
    let out = assemble_utf8_chunks(&[&snowman[..4], &snowman[4..]]).unwrap();
    assert_eq!(out, "hi \u{2603}");
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn gemini_headless_timeout_returns_error_for_hung_child() {
    use crate::core::llm::{CompletionRequest, LlmBackendConfig};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let cmd = dir.path().join("fake-gemini");
    fs::write(&cmd, "#!/bin/sh\nsleep 5\n").unwrap();
    let mut perms = fs::metadata(&cmd).unwrap().permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&cmd, perms).unwrap();

    let mut req = CompletionRequest::new("hello");
    req.backend = LlmBackendConfig {
        kind: crate::core::llm::LlmBackendKind::GeminiHeadless,
        gemini_cmd: cmd.display().to_string(),
        gemini_model: None,
        gemini_home: Some(dir.path().to_path_buf()),
        openai_base_url: None,
        openai_api_key: None,
        openai_model: None,
        completion_concurrency: 1,
        completion_timeout_secs: 1,
        configured: true,
    };

    let err = complete_text(req)
        .await
        .expect_err("hung child should time out");
    assert!(err.to_string().contains("timed out"));
}
