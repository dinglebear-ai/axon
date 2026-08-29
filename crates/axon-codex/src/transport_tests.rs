use super::*;

#[cfg(unix)]
fn trusted_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn starts_initializes_requests_and_stops_fake_server() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let root = trusted_tempdir();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(
        &binary,
        r#"#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    message=json.loads(line)
    if "id" not in message: continue
    method=message["method"]
    result={"userAgent":"fake"} if method=="initialize" else {"models":[{"id":"gpt-test"}]}
    print(json.dumps({"id":message["id"],"result":result}), flush=True)
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let config = ControlConfig {
        enabled: true,
        codex_binary: binary,
        control_home: home,
        request_timeout: Duration::from_secs(2),
        read_concurrency: 2,
        max_restart_backoff: Duration::from_secs(30),
    };
    let transport = ControlTransport::start(&config, RuntimeEpoch(1))
        .await
        .unwrap();
    let result = transport.request("model/list", json!({})).await.unwrap();
    assert_eq!(result["models"][0]["id"], "gpt-test");
    transport.stop().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn records_redacted_stderr_unknown_frames_and_response_correlation_failures() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let root = trusted_tempdir();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(
        &binary,
        r#"#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    message=json.loads(line)
    if message.get("method") == "initialize":
        response={"id":message["id"],"result":{"userAgent":"fake"}}
        print(json.dumps(response), flush=True)
        print(json.dumps(response), flush=True)
        print(json.dumps({"unexpected":"envelope"}), flush=True)
        print("api_key=must-not-leak", file=sys.stderr, flush=True)
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let config = ControlConfig {
        enabled: true,
        codex_binary: binary,
        control_home: home,
        request_timeout: Duration::from_secs(2),
        read_concurrency: 2,
        max_restart_backoff: Duration::from_secs(30),
    };
    let transport = ControlTransport::start(&config, RuntimeEpoch(1))
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if transport.events_after(None, 100).unwrap().len() >= 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let details = transport
        .events_after(None, 100)
        .unwrap()
        .into_iter()
        .filter_map(|event| match event.event {
            EventKind::ProtocolFailure { detail } => Some(detail),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("failed to correlate"))
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("unrecognized JSON-RPC"))
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("stderr: [REDACTED]"))
    );
    assert!(
        details
            .iter()
            .all(|detail| !detail.contains("must-not-leak"))
    );
    transport.stop().await.unwrap();
}

#[test]
fn stderr_redaction_is_bounded_and_conservative() {
    assert_eq!(redact_stderr("Authorization: Bearer abc"), "[REDACTED]");
    assert_eq!(
        redact_stderr("ordinary diagnostic\r\n"),
        "ordinary diagnostic"
    );
    assert_eq!(
        redact_stderr("https://example.com/path?token=abc"),
        "[REDACTED]"
    );
}

#[test]
fn server_request_claim_is_concurrency_safe_and_write_failure_is_retryable() {
    let mut registry = HashMap::from([(
        77,
        PendingServerRequest {
            method: "execCommandApproval".to_string(),
            expires_at: Instant::now() + Duration::from_secs(30),
            claimed: false,
            event: RecordedEvent {
                cursor: crate::events::EventCursor {
                    boot_id: 1,
                    sequence: 1,
                },
                event: EventKind::ServerRequest {
                    request_id: 77,
                    method: "execCommandApproval".to_string(),
                    params: json!({}),
                },
            },
        },
    )]);

    claim_server_request(&mut registry, 77).unwrap();
    assert_eq!(
        claim_server_request(&mut registry, 77).unwrap_err(),
        "server request response is already in progress"
    );
    finish_server_request(&mut registry, 77, false);
    claim_server_request(&mut registry, 77).unwrap();
    finish_server_request(&mut registry, 77, true);
    assert!(!registry.contains_key(&77));
}

#[test]
fn typed_server_request_results_are_method_correct() {
    assert_eq!(
        server_request_result("item/tool/requestUserInput", false, None).unwrap(),
        json!({"answers": {}})
    );
    assert!(server_request_result("item/tool/requestUserInput", true, None).is_err());
    assert_eq!(
        server_request_result(
            "mcpServer/elicitation/request",
            true,
            Some(json!({"action":"accept","content":{"name":"value"}})),
        )
        .unwrap(),
        json!({"action":"accept","content":{"name":"value"}})
    );
}

#[test]
fn overflow_rejections_are_method_correct_for_interactive_requests() {
    assert_eq!(
        server_request_result("item/tool/requestUserInput", false, None).unwrap(),
        json!({"answers": {}})
    );
    assert_eq!(
        server_request_result("mcpServer/elicitation/request", false, None).unwrap(),
        json!({"action": "decline"})
    );
    assert_eq!(
        server_request_result("execCommandApproval", false, None).unwrap(),
        json!({"decision": "decline"})
    );
}
