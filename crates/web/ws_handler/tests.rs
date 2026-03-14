use super::*;
use std::time::Duration;

#[test]
fn test_permission_response_cross_session_isolation() {
    let map: PermissionResponderMap = Arc::new(DashMap::new());

    let shared_tool_call_id = "tc-shared";

    // Insert two entries with different session_ids but the same tool_call_id.
    let (tx_a, mut rx_a) = tokio::sync::oneshot::channel::<String>();
    let (tx_b, _rx_b) = tokio::sync::oneshot::channel::<String>();
    map.insert(
        ("session-A".to_string(), shared_tool_call_id.to_string()),
        tx_a,
    );
    map.insert(
        ("session-B".to_string(), shared_tool_call_id.to_string()),
        tx_b,
    );

    assert_eq!(map.len(), 2, "Both entries should exist before routing");

    // Build a minimal WsConnState-like setup for testing the per-connection
    // permission_responders path (the H-8 ownership check is not exercised
    // here — this path is for sessions originating on the current connection).
    // We call remove() on the map directly to mirror the pre-H-8 code path.
    let removed = map.remove(&("session-A".to_string(), shared_tool_call_id.to_string()));
    if let Some((_, sender)) = removed {
        let _ = sender.send("allow".to_string());
    }

    // Session A's entry should be consumed and the value received.
    assert_eq!(map.len(), 1, "Only session A's entry should be consumed");
    assert!(
        !map.contains_key(&("session-A".to_string(), shared_tool_call_id.to_string())),
        "Session A entry should be removed"
    );
    assert!(
        map.contains_key(&("session-B".to_string(), shared_tool_call_id.to_string())),
        "Session B entry must remain untouched"
    );

    // Verify session A received the correct value.
    let received = rx_a
        .try_recv()
        .expect("Session A should have received the response");
    assert_eq!(received, "allow");
}

#[test]
fn execute_message_accepts_exec_id_alias() {
    let parsed: WsClientMsg = serde_json::from_str(
        r#"{"type":"execute","mode":"query","input":"rust","exec_id":"ws-exec-42"}"#,
    )
    .expect("execute message should deserialize");

    assert_eq!(parsed.msg_type, "execute");
    assert_eq!(parsed.mode, "query");
    assert_eq!(parsed.input, "rust");
    assert_eq!(parsed.id, "ws-exec-42");
}

#[test]
fn cancel_message_still_accepts_id_field() {
    let parsed: WsClientMsg =
        serde_json::from_str(r#"{"type":"cancel","mode":"crawl","id":"job-123"}"#)
            .expect("cancel message should deserialize");

    assert_eq!(parsed.msg_type, "cancel");
    assert_eq!(parsed.mode, "crawl");
    assert_eq!(parsed.id, "job-123");
}

#[test]
fn acp_resume_result_ok_key_is_serialized_correctly() {
    // Regression for C-1: verify "ok" (not "success") is emitted.
    // Uses the production acp_resume_json() serializer so the test catches
    // any key rename (e.g. "ok" → "success") in the actual code path.
    let msg = acp_resume_json(true, "sess-123", None, Some(5));

    assert!(msg.contains("\"ok\":true"), "must use 'ok' key, got: {msg}");
    assert!(
        !msg.contains("\"success\""),
        "must NOT use 'success' key, got: {msg}"
    );

    // Verify all expected fields are present.
    let parsed: serde_json::Value =
        serde_json::from_str(&msg).expect("acp_resume_json must produce valid JSON");
    assert_eq!(parsed["type"], "acp_resume_result");
    assert_eq!(parsed["session_id"], "sess-123");
    assert_eq!(parsed["replayed"], 5);
}

#[test]
fn rate_limit_constants_are_sane() {
    assert_eq!(RATE_LIMIT_WINDOW_SECS, 60);
    assert_eq!(RATE_LIMIT_MAX_EXECUTES, 120);
    assert_eq!(RATE_LIMIT_MAX_READ_FILE, 60);
}

#[test]
fn check_rate_limit_resets_after_window() {
    let limiter: DashMap<IpAddr, (u32, Instant, u32, Instant)> = DashMap::new();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();

    // Fill up execute quota.
    for _ in 0..RATE_LIMIT_MAX_EXECUTES {
        assert!(check_rate_limit(&limiter, ip, RateLimitCategory::Execute));
    }
    // Next one should be denied.
    assert!(!check_rate_limit(&limiter, ip, RateLimitCategory::Execute));

    // Simulate window expiry by backdating the execute window start.
    limiter
        .entry(ip)
        .and_modify(|(_, exec_start, _, _read_start)| {
            *exec_start = Instant::now() - Duration::from_secs(RATE_LIMIT_WINDOW_SECS + 1);
        });

    // After window reset, should be allowed again.
    assert!(check_rate_limit(&limiter, ip, RateLimitCategory::Execute));
}

#[test]
fn check_rate_limit_separate_counters_per_category() {
    let limiter: DashMap<IpAddr, (u32, Instant, u32, Instant)> = DashMap::new();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();

    // Exhaust execute quota.
    for _ in 0..RATE_LIMIT_MAX_EXECUTES {
        check_rate_limit(&limiter, ip, RateLimitCategory::Execute);
    }
    assert!(!check_rate_limit(&limiter, ip, RateLimitCategory::Execute));

    // read_file should still be allowed (separate counter).
    assert!(check_rate_limit(&limiter, ip, RateLimitCategory::ReadFile));
}

#[test]
fn msg_type_probe_detects_crawl_files() {
    let msg = r#"{"type":"crawl_files","output_dir":"/tmp/out","job_id":"j1"}"#;
    let probe = serde_json::from_str::<MsgType>(msg).unwrap();
    assert_eq!(probe.msg_type, "crawl_files");
}

#[test]
fn msg_type_probe_ignores_crawl_files_in_content() {
    // P1-8: a message with "crawl_files" in the content but a different type field
    // must NOT be detected as a crawl_files message.
    let msg = r#"{"type":"command.output.line","line":"found crawl_files in text"}"#;
    let probe = serde_json::from_str::<MsgType>(msg).unwrap();
    assert_ne!(probe.msg_type, "crawl_files");
}
