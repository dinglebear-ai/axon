use super::*;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn no_delta(_: &str) -> Result<(), BoxError> {
    Ok(())
}

fn new_state() -> CodexStreamState {
    CodexStreamState::new(
        Some("gpt-5.5".to_string()),
        None,
        "Summarize the corpus.",
        None,
        "/tmp/cwd",
        "9.9.9",
    )
}

#[test]
fn initialize_line_omits_jsonrpc_header_and_sets_client() {
    let line = initialize_line("1.2.3");
    let value: Value = serde_json::from_str(&line).unwrap();
    assert!(value.get("jsonrpc").is_none(), "wire omits jsonrpc header");
    assert_eq!(value["method"], "initialize");
    assert_eq!(value["params"]["clientInfo"]["name"], "axon");
    assert_eq!(value["params"]["clientInfo"]["version"], "1.2.3");
    assert!(value["params"]["capabilities"].is_null());
}

#[test]
fn thread_start_lines_set_safe_synthesis_policy() {
    let lines = thread_start_lines(Some("gpt-5.5"), "/tmp/cwd", None);
    assert_eq!(lines.len(), 2);
    let initialized: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(initialized["method"], "initialized");
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(start["method"], "thread/start");
    assert_eq!(start["params"]["approvalPolicy"], "never");
    assert_eq!(start["params"]["sandbox"], "read-only");
    assert_eq!(start["params"]["model"], "gpt-5.5");
    assert_eq!(start["params"]["cwd"], "/tmp/cwd");
}

#[test]
fn thread_start_sets_ephemeral_true() {
    let lines = thread_start_lines(Some("gpt-5.5"), "/tmp/cwd", None);
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(
        start["params"]["ephemeral"], true,
        "ephemeral must be true to skip session persistence"
    );
}

#[test]
fn thread_start_sends_developer_instructions_when_system_prompt_set() {
    let lines = thread_start_lines(
        Some("gpt-5.5"),
        "/tmp/cwd",
        Some("You are a helpful assistant."),
    );
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(
        start["params"]["developerInstructions"],
        "You are a helpful assistant."
    );
    assert_eq!(
        start["params"]["baseInstructions"],
        "You are a helpful assistant."
    );
}

#[test]
fn thread_start_omits_developer_instructions_when_no_system_prompt() {
    let lines = thread_start_lines(Some("gpt-5.5"), "/tmp/cwd", None);
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert!(start["params"].get("developerInstructions").is_none());
    assert!(start["params"].get("baseInstructions").is_none());
}

#[test]
fn thread_start_omits_blank_model() {
    let lines = thread_start_lines(Some("   "), "/tmp/cwd", None);
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert!(start["params"].get("model").is_none());
}

#[test]
fn turn_start_carries_effort_when_set() {
    let line = turn_start_line("thr_abc", "Do the thing.", Some("high"));
    let turn: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(turn["params"]["effort"], "high");
}

#[test]
fn reasoning_effort_wire_forms_are_stable() {
    assert_eq!(ReasoningEffort::Low.as_wire(), "low");
    assert_eq!(ReasoningEffort::Medium.as_wire(), "medium");
    assert_eq!(ReasoningEffort::High.as_wire(), "high");
}

#[test]
fn from_request_maps_fields_by_name() {
    // Guards against silent positional transposition: each CompletionRequest
    // field must land in the matching CodexStreamState field.
    let req = CompletionRequest::new("user question")
        .system_prompt("system instructions")
        .effort(ReasoningEffort::High);
    let state = CodexStreamState::from_request(&req, "/work/dir", "1.2.3");
    assert_eq!(state.user_prompt, "user question");
    assert_eq!(state.system_prompt.as_deref(), Some("system instructions"));
    assert_eq!(state.effort, Some(ReasoningEffort::High));
    assert_eq!(state.cwd, "/work/dir");
    assert_eq!(state.version, "1.2.3");
    // No model configured → resolution leaves it unset rather than inventing one.
    assert_eq!(state.model, None);
}

#[test]
fn turn_start_omits_effort_when_none() {
    let line = turn_start_line("thr_abc", "Do the thing.", None);
    let turn: Value = serde_json::from_str(&line).unwrap();
    assert!(turn["params"].get("effort").is_none());
}

#[test]
fn turn_start_input_is_user_prompt_only() {
    let line = turn_start_line("thr_abc", "user content only", Some("medium"));
    let turn: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(turn["params"]["input"][0]["text"], "user content only");
    // The system prompt must NOT be folded into input.
    let input_text = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert!(
        !input_text.contains("system"),
        "system prompt must not appear in input"
    );
}

#[test]
fn system_prompt_routed_to_developer_instructions_not_joined_prompt() {
    // Build a state with system_prompt; after init ack, thread/start must carry
    // developerInstructions and turn/start input must be the user_prompt alone.
    let mut state = CodexStreamState::new(
        Some("gpt-5.5".to_string()),
        Some("Be concise.".to_string()),
        "What is 2+2?",
        Some(ReasoningEffort::Medium),
        "/tmp/cwd",
        "9.9.9",
    );
    // Simulate init ack → should produce thread/start.
    let step = state
        .handle_line(r#"{"id":0,"result":{"userAgent":"x"}}"#, &mut no_delta)
        .unwrap();
    let thread_start_json = match step {
        CodexStep::Send(ref lines) => {
            assert_eq!(lines.len(), 2);
            serde_json::from_str::<Value>(&lines[1]).unwrap()
        }
        other => panic!("expected Send for thread/start, got {other:?}"),
    };
    assert_eq!(
        thread_start_json["params"]["developerInstructions"],
        "Be concise."
    );
    assert_eq!(thread_start_json["params"]["ephemeral"], true);

    // Simulate thread/start ack → should produce turn/start.
    let step = state
        .handle_line(
            r#"{"id":1,"result":{"thread":{"id":"thr_xyz"},"model":"gpt-5.5"}}"#,
            &mut no_delta,
        )
        .unwrap();
    let turn_json = match step {
        CodexStep::Send(ref lines) => {
            assert_eq!(lines.len(), 1);
            serde_json::from_str::<Value>(&lines[0]).unwrap()
        }
        other => panic!("expected Send for turn/start, got {other:?}"),
    };
    assert_eq!(turn_json["params"]["input"][0]["text"], "What is 2+2?");
    assert_eq!(turn_json["params"]["effort"], "medium");
    // The system prompt must not appear in the user input.
    let input_text = turn_json["params"]["input"][0]["text"].as_str().unwrap();
    assert!(
        !input_text.contains("Be concise."),
        "system prompt must not be in turn input"
    );
}

#[test]
fn init_response_triggers_thread_start() {
    let mut state = new_state();
    let step = state
        .handle_line(r#"{"id":0,"result":{"userAgent":"x"}}"#, &mut no_delta)
        .unwrap();
    match step {
        CodexStep::Send(lines) => {
            assert_eq!(lines.len(), 2);
            assert!(lines[1].contains("thread/start"));
        }
        other => panic!("expected Send, got {other:?}"),
    }
}

#[test]
fn thread_response_triggers_turn_start_with_thread_id() {
    let mut state = new_state();
    let step = state
        .handle_line(
            r#"{"id":1,"result":{"thread":{"id":"thr_abc"},"model":"gpt-5.5"}}"#,
            &mut no_delta,
        )
        .unwrap();
    match step {
        CodexStep::Send(lines) => {
            assert_eq!(lines.len(), 1);
            let turn: Value = serde_json::from_str(&lines[0]).unwrap();
            assert_eq!(turn["method"], "turn/start");
            assert_eq!(turn["params"]["threadId"], "thr_abc");
            assert_eq!(turn["params"]["input"][0]["text"], "Summarize the corpus.");
        }
        other => panic!("expected Send, got {other:?}"),
    }
}

#[test]
fn missing_thread_id_is_an_error() {
    let mut state = new_state();
    let err = state
        .handle_line(r#"{"id":1,"result":{"thread":{}}}"#, &mut no_delta)
        .unwrap_err();
    assert!(err.to_string().contains("thread.id"));
}

#[test]
fn response_error_propagates() {
    let mut state = new_state();
    let err = state
        .handle_line(r#"{"id":0,"error":{"message":"boom"}}"#, &mut no_delta)
        .unwrap_err();
    assert!(err.to_string().contains("boom"));
}

#[test]
fn response_error_redacts_compact_json_secret_message() {
    let mut state = CodexStreamState::new(None, None, "prompt", None, "/tmp", "test");
    let err = state
        .handle_line(
            r#"{"id":0,"error":{"message":"{\"api_key\":\"sk-protocol-secret\",\"token\":\"atk_protocol_secret\",\"message\":\"bad auth\"}"}}"#,
            &mut no_delta,
        )
        .unwrap_err();
    let text = err.to_string();

    assert!(text.contains("bad auth"), "got: {text}");
    assert!(text.contains("[REDACTED]"), "got: {text}");
    assert!(!text.contains("sk-protocol-secret"), "got: {text}");
    assert!(!text.contains("atk_protocol_secret"), "got: {text}");
}

#[test]
fn malformed_protocol_error_is_bounded_and_redacted() {
    let mut state = CodexStreamState::new(None, None, "prompt", None, "/tmp", "test");
    let secret = format!(
        "{{ bad json {} }}",
        "sk-abcdefghijklmnopqrstuvwxyz0123456789"
    );

    let err = state.handle_line(&secret, &mut no_delta).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("[REDACTED]"));
    assert!(text.len() < 600);
    assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
}

#[test]
fn malformed_protocol_error_truncates_on_utf8_boundary() {
    let mut state = CodexStreamState::new(None, None, "prompt", None, "/tmp", "test");
    let line = format!("{{ bad json {} }}", "é".repeat(PROTOCOL_ERROR_LIMIT));

    let err = state.handle_line(&line, &mut no_delta).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("..."), "got: {text}");
    assert!(text.len() < 700, "got {} bytes", text.len());
}

#[test]
fn json_rpc_error_is_summarized_without_raw_payload_echo() {
    let mut state = CodexStreamState::new(None, None, "prompt", None, "/tmp", "test");
    let line = r#"{"id":2,"error":{"message":"failed with sk-abcdefghijklmnopqrstuvwxyz0123456789","data":{"prompt":"secret prompt"}}}"#;

    let err = state.handle_line(line, &mut no_delta).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains("secret prompt"));
    assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
}

#[test]
fn json_rpc_error_truncates_on_utf8_boundary() {
    let mut state = CodexStreamState::new(None, None, "prompt", None, "/tmp", "test");
    let message = "🚀".repeat(PROTOCOL_ERROR_LIMIT);
    let line = serde_json::json!({
        "id": 2,
        "error": {
            "message": message,
        }
    })
    .to_string();

    let err = state.handle_line(&line, &mut no_delta).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("..."), "got: {text}");
    assert!(text.len() < 700, "got {} bytes", text.len());
}

#[test]
fn deltas_accumulate_and_invoke_callback() {
    let mut state = new_state();
    let mut seen = String::new();
    let mut sink = |d: &str| {
        seen.push_str(d);
        Ok(())
    };
    for delta in ["Hel", "lo ", "world"] {
        let line = format!(
            r#"{{"method":"item/agentMessage/delta","params":{{"threadId":"t","turnId":"u","itemId":"i","delta":"{delta}"}}}}"#
        );
        assert_eq!(
            state.handle_line(&line, &mut sink).unwrap(),
            CodexStep::Continue
        );
    }
    assert_eq!(seen, "Hello world");
    let response = state.into_response().unwrap();
    assert_eq!(response.text, "Hello world");
}

#[test]
fn delta_callback_error_propagates() {
    let mut state = new_state();
    let line = r#"{"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"u","itemId":"i","delta":"hi"}}"#;
    let mut failing = |_: &str| Err::<(), BoxError>("sink closed".into());
    let err = state.handle_line(line, &mut failing).unwrap_err();
    assert!(err.to_string().contains("sink closed"), "got: {err}");
}

#[test]
fn ignores_reasoning_and_user_message_items() {
    let mut state = new_state();
    for line in [
        r#"{"method":"item/started","params":{"item":{"type":"reasoning","id":"r"}}}"#,
        r#"{"method":"item/completed","params":{"item":{"type":"userMessage","id":"u","text":""}}}"#,
        r#"{"method":"thread/status/changed","params":{"status":{"type":"active"}}}"#,
        r#"{"method":"mcpServer/startupStatus/updated","params":{"name":"codex_apps","status":"ready"}}"#,
    ] {
        assert_eq!(
            state.handle_line(line, &mut no_delta).unwrap(),
            CodexStep::Continue
        );
    }
}

#[test]
fn final_item_text_used_when_no_deltas() {
    let mut state = new_state();
    let line = r#"{"method":"item/completed","params":{"item":{"type":"agentMessage","id":"m","text":"final answer","phase":"final_answer"}}}"#;
    state.handle_line(line, &mut no_delta).unwrap();
    let response = state.into_response().unwrap();
    assert_eq!(response.text, "final answer");
}

#[test]
fn token_usage_is_captured() {
    let mut state = new_state();
    let line = r#"{"method":"thread/tokenUsage/updated","params":{"tokenUsage":{"total":{"totalTokens":100,"inputTokens":80,"outputTokens":20}}}}"#;
    state.handle_line(line, &mut no_delta).unwrap();
    state
        .handle_line(
            r#"{"method":"item/agentMessage/delta","params":{"delta":"x"}}"#,
            &mut no_delta,
        )
        .unwrap();
    let response = state.into_response().unwrap();
    let usage = response.usage.expect("usage captured");
    assert_eq!(usage.prompt_tokens, 80);
    assert_eq!(usage.completion_tokens, 20);
    assert_eq!(usage.total_tokens, 100);
}

#[test]
fn turn_completed_success_is_done() {
    let mut state = new_state();
    let step = state
        .handle_line(
            r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"status":"completed","error":null}}}"#,
            &mut no_delta,
        )
        .unwrap();
    assert_eq!(step, CodexStep::Done);
}

#[test]
fn turn_completed_failed_surfaces_error_message() {
    let mut state = new_state();
    let err = state
        .handle_line(
            r#"{"method":"turn/completed","params":{"turn":{"status":"failed","error":{"message":"tool incompatible with reasoning.effort"}}}}"#,
            &mut no_delta,
        )
        .unwrap_err();
    assert!(err.to_string().contains("tool incompatible"));
}

#[test]
fn fatal_error_notification_fails_fast() {
    let mut state = new_state();
    let err = state
        .handle_line(
            r#"{"method":"error","params":{"error":{"message":"400 bad request"},"willRetry":false,"threadId":"t","turnId":"u"}}"#,
            &mut no_delta,
        )
        .unwrap_err();
    assert!(err.to_string().contains("400 bad request"));
}

#[test]
fn retryable_error_is_remembered_not_fatal() {
    let mut state = new_state();
    let step = state
        .handle_line(
            r#"{"method":"error","params":{"error":{"message":"transient"},"willRetry":true}}"#,
            &mut no_delta,
        )
        .unwrap();
    assert_eq!(step, CodexStep::Continue);
    let err = state
        .handle_line(
            r#"{"method":"turn/completed","params":{"turn":{"status":"failed","error":null}}}"#,
            &mut no_delta,
        )
        .unwrap_err();
    assert!(err.to_string().contains("transient"));
}

#[test]
fn server_request_is_declined_to_avoid_deadlock() {
    let mut state = new_state();
    let step = state
        .handle_line(
            r#"{"method":"item/commandExecution/requestApproval","id":42,"params":{}}"#,
            &mut no_delta,
        )
        .unwrap();
    match step {
        CodexStep::Send(lines) => {
            let reply: Value = serde_json::from_str(&lines[0]).unwrap();
            assert_eq!(reply["id"], 42);
            assert!(reply.get("error").is_some());
        }
        other => panic!("expected Send(error reply), got {other:?}"),
    }
}

#[test]
fn empty_lines_are_ignored() {
    let mut state = new_state();
    assert_eq!(
        state.handle_line("   ", &mut no_delta).unwrap(),
        CodexStep::Continue
    );
}

#[test]
fn malformed_json_is_an_error() {
    let mut state = new_state();
    let err = state.handle_line("{not json", &mut no_delta).unwrap_err();
    assert!(err.to_string().contains("malformed"));
}

#[test]
fn no_answer_text_is_an_error() {
    let state = new_state();
    let err = state.into_response().unwrap_err();
    assert!(err.to_string().contains("no answer text"));
}
