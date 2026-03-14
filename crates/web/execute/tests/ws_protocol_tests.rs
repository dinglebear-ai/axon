//! Unit tests for WebSocket protocol parsing, allowlist enforcement, and ANSI stripping.
//!
//! Covers security-sensitive surface of the WS bridge that `ws_event_v2_tests.rs` does not:
//! - Inbound message JSON shape (deserialization of execute / cancel / unknown types)
//! - `ALLOWED_MODES` integrity — safe commands present, shell-injection attempts absent
//! - `ALLOWED_FLAGS` enforcement — `build_args` drops keys absent from the allowlist
//! - ANSI escape code stripping via `strip_ansi`
//! - `build_args` argument-building edge cases (input sanitisation, json flag, scrape defaults)
//!
//! No live WebSocket connection or subprocess required — all tests are pure unit tests.

use serde_json::{Value, json};

// ── WsClientMsg inbound JSON shape ────────────────────────────────────────────

#[test]
fn execute_message_json_shape_round_trips() {
    // Verify the expected wire shape for an `execute` message survives a
    // serde_json round-trip with all required fields intact.
    let raw = json!({
        "type": "execute",
        "mode": "scrape",
        "input": "https://example.com"
    });
    let serialized = serde_json::to_string(&raw).expect("json should serialize");
    let parsed: Value = serde_json::from_str(&serialized).expect("json should parse");

    assert_eq!(
        parsed.get("type").and_then(Value::as_str),
        Some("execute"),
        "type field must survive round-trip"
    );
    assert_eq!(
        parsed.get("mode").and_then(Value::as_str),
        Some("scrape"),
        "mode field must survive round-trip"
    );
    assert_eq!(
        parsed.get("input").and_then(Value::as_str),
        Some("https://example.com"),
        "input field must survive round-trip"
    );
}

#[test]
fn cancel_message_json_shape_round_trips() {
    let raw = json!({
        "type": "cancel",
        "mode": "crawl",
        "id": "550e8400-e29b-41d4-a716-446655440000"
    });
    let serialized = serde_json::to_string(&raw).expect("json should serialize");
    let parsed: Value = serde_json::from_str(&serialized).expect("json should parse");

    assert_eq!(
        parsed.get("type").and_then(Value::as_str),
        Some("cancel"),
        "type field must survive round-trip"
    );
    assert_eq!(
        parsed.get("id").and_then(Value::as_str),
        Some("550e8400-e29b-41d4-a716-446655440000"),
        "id field must survive round-trip"
    );
}

#[test]
fn unknown_message_type_is_parseable_json_but_not_a_handled_type() {
    // The WS read loop silently drops unrecognised types (`_ => {}`).
    // Verify the JSON shape is at least parseable so the router sees the `type` field.
    let raw = json!({"type": "unknown_command", "payload": "whatever"});
    let serialized = serde_json::to_string(&raw).expect("json should serialize");
    let parsed: Value = serde_json::from_str(&serialized).expect("json should parse");

    let msg_type = parsed.get("type").and_then(Value::as_str);
    assert_eq!(msg_type, Some("unknown_command"));
    // Must not match any handled type — router would drop it silently.
    assert!(!matches!(
        msg_type,
        Some("execute" | "cancel" | "read_file")
    ));
}

#[test]
fn malformed_json_fails_to_parse() {
    // Mirrors the `let Ok(...) else { send error }` branch in the WS read loop.
    let result = serde_json::from_str::<Value>("not valid json {{{{");
    assert!(result.is_err(), "malformed JSON must fail to parse");
}

// ── ALLOWED_MODES integrity ───────────────────────────────────────────────────

#[test]
fn allowed_modes_contains_expected_safe_subcommands() {
    let modes = super::allowed_modes();
    for expected in &[
        "scrape",
        "crawl",
        "map",
        "query",
        "ask",
        "stats",
        "sources",
        "pulse_chat",
    ] {
        assert!(
            modes.contains(expected),
            "ALLOWED_MODES must contain '{expected}'"
        );
    }
}

#[test]
fn allowed_modes_rejects_shell_injection_candidates() {
    // These strings must NEVER appear in ALLOWED_MODES — they would enable arbitrary execution.
    let modes = super::allowed_modes();
    for forbidden in &[
        "rm",
        "sh",
        "bash",
        "exec",
        "__proto__",
        "",
        "drop",
        "../etc",
    ] {
        assert!(
            !modes.contains(forbidden),
            "ALLOWED_MODES must NOT contain '{forbidden}'"
        );
    }
}

#[test]
fn allowed_modes_no_entry_starts_with_dash() {
    // A mode starting with `--` could be misinterpreted as a CLI flag by the subprocess.
    let modes = super::allowed_modes();
    for mode in modes {
        assert!(
            !mode.starts_with('-'),
            "ALLOWED_MODES entry '{mode}' must not start with '-'"
        );
    }
}

// ── ALLOWED_FLAGS list integrity ──────────────────────────────────────────────

#[test]
fn allowed_flags_all_cli_flags_start_with_double_dash() {
    for (json_key, cli_flag) in super::allowed_flags() {
        assert!(
            cli_flag.starts_with("--"),
            "ALLOWED_FLAGS ({json_key}, {cli_flag}): CLI flag must start with '--'"
        );
    }
}

#[test]
fn allowed_flags_no_duplicate_json_keys() {
    // Duplicate json_key entries would cause silent shadowing in build_args.
    let mut seen = std::collections::HashSet::new();
    for (json_key, _) in super::allowed_flags() {
        assert!(
            seen.insert(*json_key),
            "ALLOWED_FLAGS has duplicate json_key '{json_key}'"
        );
    }
}

// ── build_args: flag filtering ────────────────────────────────────────────────

#[test]
fn build_args_includes_known_flags_by_name() {
    let flags = json!({"limit": 20, "collection": "my_col"});
    let args = super::build_args("query", "rust async", &flags);

    assert!(
        args.contains(&"--limit".to_string()),
        "known flag '--limit' must appear in args"
    );
    assert!(
        args.contains(&"20".to_string()),
        "numeric flag value '20' must appear in args"
    );
    assert!(
        args.contains(&"--collection".to_string()),
        "known flag '--collection' must appear in args"
    );
    assert!(
        args.contains(&"my_col".to_string()),
        "string flag value 'my_col' must appear in args"
    );
}

#[test]
fn build_args_drops_unknown_flags_silently() {
    // Keys not in ALLOWED_FLAGS must be dropped — they could inject arbitrary flags.
    let flags = json!({
        "rm": true,
        "exec": "/bin/sh",
        "shell": true,
        "limit": 5
    });
    let args = super::build_args("query", "test", &flags);

    for forbidden_flag in &["--rm", "--exec", "--shell"] {
        assert!(
            !args.contains(&forbidden_flag.to_string()),
            "unknown flag '{forbidden_flag}' must be silently dropped"
        );
    }
    assert!(
        args.contains(&"--limit".to_string()),
        "known flag '--limit' must survive filtering"
    );
}

#[test]
fn build_args_drops_empty_string_flag_values() {
    // An empty string flag value must not emit the corresponding CLI flag.
    let flags = json!({"collection": ""});
    let args = super::build_args("query", "test", &flags);
    assert!(
        !args.contains(&"--collection".to_string()),
        "empty string flag value must not emit '--collection'"
    );
}

#[test]
fn build_args_bool_false_emits_flag_followed_by_false_literal() {
    let flags = json!({"embed": false});
    let args = super::build_args("query", "test", &flags);

    let embed_pos = args.iter().position(|a| a == "--embed");
    assert!(
        embed_pos.is_some(),
        "--embed flag must be present for bool false value"
    );
    if let Some(pos) = embed_pos {
        assert_eq!(
            args.get(pos + 1).map(String::as_str),
            Some("false"),
            "--embed must be followed by literal 'false'"
        );
    }
}

// ── build_args: mode-specific behaviour ──────────────────────────────────────

#[test]
fn build_args_adds_json_flag_for_non_search_modes() {
    let flags = json!({});
    for mode in &["query", "ask", "scrape", "crawl", "stats"] {
        let args = super::build_args(mode, "test", &flags);
        assert!(
            args.contains(&"--json".to_string()),
            "mode '{mode}' must include '--json' flag"
        );
    }
}

#[test]
fn search_and_research_include_json_flag_in_build_args() {
    // "search" and "research" were removed from NO_JSON_MODES because they are now routed
    // through the service layer (ServiceMode → call_search / call_research → send_json_owned)
    // and produce structured JSON regardless.  build_args is never called for these modes
    // at runtime, but if it were, --json would be included (no entry in NO_JSON_MODES).
    let flags = json!({});
    for mode in &["search", "research"] {
        let args = super::build_args(mode, "test", &flags);
        assert!(
            args.contains(&"--json".to_string()),
            "mode '{mode}' includes --json (removed from NO_JSON_MODES; service layer is authoritative)"
        );
    }
}

#[test]
fn build_args_omits_json_flag_for_evaluate_events_mode() {
    let flags = json!({"responses_mode": "events"});
    let args = super::build_args("evaluate", "What changed?", &flags);
    assert!(
        !args.contains(&"--json".to_string()),
        "evaluate events mode must NOT include '--json'"
    );
    assert!(
        args.contains(&"--responses-mode".to_string()),
        "evaluate events mode must include '--responses-mode'"
    );
    assert!(
        args.contains(&"events".to_string()),
        "evaluate events mode must pass the 'events' value"
    );
}

#[test]
fn build_args_scrape_always_appends_embed_false() {
    let flags = json!({});
    let args = super::build_args("scrape", "https://example.com", &flags);

    let embed_pos = args.iter().position(|a| a == "--embed");
    assert!(embed_pos.is_some(), "scrape must include '--embed'");
    if let Some(pos) = embed_pos {
        assert_eq!(
            args.get(pos + 1).map(String::as_str),
            Some("false"),
            "scrape '--embed' must be followed by 'false'"
        );
    }
}

// ── build_args: input sanitisation ───────────────────────────────────────────

#[test]
fn build_args_strips_leading_dashes_from_url_input() {
    // A leading `--` in the URL would inject a flag into the subprocess command line.
    let flags = json!({});
    let args = super::build_args("scrape", "--malicious-url", &flags);

    assert!(
        !args.contains(&"--malicious-url".to_string()),
        "leading dashes must be stripped from input to prevent flag injection"
    );
    assert!(
        args.contains(&"malicious-url".to_string()),
        "stripped input must still appear as a positional argument"
    );
}

#[test]
fn build_args_skips_wait_flag_for_async_modes() {
    // Direct async modes (fire-and-forget) suppress --wait since they enqueue and return.
    let flags = json!({"wait": true});
    for async_mode in &["crawl", "extract", "embed", "github", "reddit", "youtube"] {
        let args = super::build_args(async_mode, "test", &flags);
        assert!(
            !args.contains(&"--wait".to_string()),
            "async mode '{async_mode}' must suppress '--wait'"
        );
    }
}

#[test]
fn build_args_allows_wait_flag_for_sync_modes() {
    let flags = json!({"wait": true});
    for sync_mode in &["scrape", "query", "ask"] {
        let args = super::build_args(sync_mode, "test", &flags);
        assert!(
            args.contains(&"--wait".to_string()),
            "sync mode '{sync_mode}' must allow '--wait'"
        );
    }
}

// ── Sync mode routing ─────────────────────────────────────────────────────────

#[test]
fn direct_sync_modes_are_not_async_modes() {
    // A mode cannot be in both lists — that would create ambiguous routing.
    let async_modes = super::async_modes();
    for mode in super::direct_sync_modes() {
        assert!(
            !async_modes.contains(mode),
            "mode '{mode}' must not appear in both direct_sync_modes and async_modes"
        );
    }
}

#[test]
fn direct_sync_modes_all_present_in_allowed_modes() {
    let allowed = super::allowed_modes();
    for mode in super::direct_sync_modes() {
        assert!(
            allowed.contains(mode),
            "direct_sync mode '{mode}' must also be present in ALLOWED_MODES"
        );
    }
}

#[test]
fn async_modes_all_present_in_allowed_modes() {
    let allowed = super::allowed_modes();
    for mode in super::async_modes() {
        assert!(
            allowed.contains(mode),
            "async mode '{mode}' must also be present in ALLOWED_MODES"
        );
    }
}

#[test]
fn direct_sync_modes_contains_core_service_modes() {
    let direct = super::direct_sync_modes();
    for expected in &[
        "scrape",
        "map",
        "query",
        "retrieve",
        "ask",
        "search",
        "research",
        "stats",
        "sources",
        "domains",
        "doctor",
        "status",
        "pulse_chat",
        "suggest",
        "evaluate",
        "dedupe",
        "screenshot",
        "debug",
        "sessions",
    ] {
        assert!(
            direct.contains(expected),
            "direct_sync_modes must contain '{expected}'"
        );
    }
}

#[test]
fn no_sync_service_modes_remain_on_subprocess_fallback() {
    let direct = super::direct_sync_modes();
    let async_m = super::async_modes();
    for mode in &["sessions", "debug"] {
        assert!(
            direct.contains(mode),
            "mode '{mode}' should now dispatch directly through services"
        );
        assert!(
            !async_m.contains(mode),
            "mode '{mode}' must not be reclassified as async"
        );
    }
}

// ── ANSI stripping ────────────────────────────────────────────────────────────

#[test]
fn strip_ansi_removes_color_escape_codes() {
    let stripped = super::strip_ansi("\x1b[32mhello\x1b[0m");
    assert_eq!(stripped, "hello", "ANSI color codes must be stripped");
}

#[test]
fn strip_ansi_removes_bold_and_reset_sequences() {
    let stripped = super::strip_ansi("\x1b[1mbold\x1b[0m normal");
    assert_eq!(
        stripped, "bold normal",
        "ANSI bold/reset codes must be stripped"
    );
}

#[test]
fn strip_ansi_leaves_plain_text_unchanged() {
    let input = "plain text without escapes";
    assert_eq!(
        super::strip_ansi(input),
        input,
        "plain text must pass through unchanged"
    );
}

#[test]
fn strip_ansi_handles_empty_string() {
    assert_eq!(
        super::strip_ansi(""),
        "",
        "empty string must remain empty after stripping"
    );
}

#[test]
fn strip_ansi_handles_multiple_sequences_on_one_line() {
    // Spinner output from indicatif commonly emits several sequences on one line.
    let input = "\x1b[2K\x1b[1G\x1b[36m⠋\x1b[0m Crawling...";
    let stripped = super::strip_ansi(input);
    assert_eq!(
        stripped, "⠋ Crawling...",
        "multiple escape sequences must all be stripped"
    );
}
