use super::*;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn no_delta(_: &str) -> Result<(), BoxError> {
    Ok(())
}

fn new_state() -> CodexStreamState {
    CodexStreamState::new(
        Some("gpt-5.5".to_string()),
        "Summarize the corpus.",
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
    let lines = thread_start_lines(Some("gpt-5.5"), "/tmp/cwd");
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
fn thread_start_omits_blank_model() {
    let lines = thread_start_lines(Some("   "), "/tmp/cwd");
    let start: Value = serde_json::from_str(&lines[1]).unwrap();
    assert!(start["params"].get("model").is_none());
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
fn malformed_protocol_error_is_bounded_and_redacted() {
    let mut state = CodexStreamState::new(None, "prompt", "/tmp", "test");
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
fn json_rpc_error_is_summarized_without_raw_payload_echo() {
    let mut state = CodexStreamState::new(None, "prompt", "/tmp", "test");
    let line = r#"{"id":2,"error":{"message":"failed with sk-abcdefghijklmnopqrstuvwxyz0123456789","data":{"prompt":"secret prompt"}}}"#;

    let err = state.handle_line(line, &mut no_delta).unwrap_err();
    let text = err.to_string();

    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains("secret prompt"));
    assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
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
    // A failing streaming sink (e.g. a closed channel) must surface as an error
    // from handle_line, not be swallowed.
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
    // need some text to build a response
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
    // If the turn then fails without its own message, the remembered one surfaces.
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
