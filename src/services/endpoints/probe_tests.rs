use super::*;
use crate::core::http::{build_client, get_allow_loopback, set_allow_loopback};
use httpmock::prelude::*;
use serial_test::serial;

struct LoopbackGuard {
    previous: bool,
}

impl LoopbackGuard {
    fn allow() -> Self {
        let previous = get_allow_loopback();
        set_allow_loopback(true);
        Self { previous }
    }
}

impl Drop for LoopbackGuard {
    fn drop(&mut self) {
        set_allow_loopback(self.previous);
    }
}

fn probe_client() -> reqwest::Client {
    build_client(PROBE_TIMEOUT_SECS, Some(axon_ua())).expect("probe client")
}

#[tokio::test]
#[serial]
async fn detects_mcp_and_replays_session_id() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200)
                .header("content-type", "application/json")
                .header("mcp-session-id", "sess-123")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "serverInfo": { "name": "demo-mcp", "version": "0.9" },
                        "capabilities": {}
                    }
                }));
        })
        .await;
    // tools/list only matches when the session id assigned by initialize is replayed.
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/mcp")
                .body_includes("tools/list")
                .header("mcp-session-id", "sess-123");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": { "tools": [ { "name": "search" }, { "name": "fetch" } ] }
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/mcp"))
        .await
        .expect("mcp probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    assert_eq!(result.transport, Some(RpcTransport::Http));
    assert_eq!(result.server_name.as_deref(), Some("demo-mcp"));
    assert_eq!(result.server_version.as_deref(), Some("0.9"));
    assert_eq!(
        result.tools,
        vec!["search".to_string(), "fetch".to_string()]
    );
}

#[tokio::test]
#[serial]
async fn parses_mcp_over_sse_streamable_http() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(concat!(
                    "event: message\n",
                    "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":",
                    "{\"serverInfo\":{\"name\":\"sse-srv\",\"version\":\"3\"},\"capabilities\":{}}}\n",
                    "\n"
                ));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/mcp"))
        .await
        .expect("sse mcp probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    assert_eq!(result.server_name.as_deref(), Some("sse-srv"));
}

#[tokio::test]
#[serial]
async fn detects_openrpc_via_rpc_discover() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/rpc").body_includes("rpc.discover");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "openrpc": "1.2.6",
                        "methods": [ { "name": "eth_call" }, { "name": "eth_blockNumber" } ]
                    }
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/rpc"))
        .await
        .expect("openrpc probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Openrpc));
    assert!(result.methods.contains(&"eth_call".to_string()));
}

#[tokio::test]
#[serial]
async fn detects_jsonrpc_via_list_methods() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/jsonrpc")
                .body_includes("system.listMethods");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": [ "system.listMethods", "ping" ]
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/jsonrpc"))
        .await
        .expect("listMethods probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Jsonrpc2));
    assert!(result.methods.contains(&"ping".to_string()));
}

#[tokio::test]
#[serial]
async fn detects_jsonrpc_via_method_not_found_code() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_includes("__axon_probe__");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 4,
                    "error": { "code": -32601, "message": "Method not found" }
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/rpc"))
        .await
        .expect("fingerprint probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Jsonrpc2));
    assert!(result.methods.is_empty());
}

#[tokio::test]
#[serial]
async fn detects_sse_transport_via_get() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // No POST mocks: every JSON-RPC POST 404s, falling through to the SSE GET probe.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/sse");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("event: ping\ndata: {}\n\n");
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/sse"))
        .await
        .expect("sse transport probe result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    assert_eq!(result.transport, Some(RpcTransport::Sse));
}

#[tokio::test]
#[serial]
async fn non_rpc_endpoint_yields_no_probe() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/api");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({ "hello": "world" }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({ "hello": "world" }));
        })
        .await;

    assert!(
        probe_one(&probe_client(), &server.url("/api"))
            .await
            .is_none()
    );
}

#[test]
fn parse_sse_event_concatenates_data_lines() {
    let block = "event: message\ndata: {\"a\":1,\ndata: \"b\":2}";
    let value = parse_sse_event(block).expect("parsed sse json");
    assert_eq!(value.get("a").and_then(Value::as_i64), Some(1));
    assert_eq!(value.get("b").and_then(Value::as_i64), Some(2));
}

#[test]
fn protocol_and_transport_as_str_match_serde() {
    for p in [
        RpcProtocol::Jsonrpc2,
        RpcProtocol::Openrpc,
        RpcProtocol::Mcp,
    ] {
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            Value::String(p.as_str().to_string())
        );
    }
    for t in [RpcTransport::Http, RpcTransport::Sse] {
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            Value::String(t.as_str().to_string())
        );
    }
}

#[test]
fn probe_timeout_clamps_to_ceiling_but_can_shorten() {
    let mut cfg = Config::test_default();
    // Unset → fixed 3s ceiling.
    cfg.request_timeout_ms = None;
    assert_eq!(probe_timeout_secs(&cfg), PROBE_TIMEOUT_SECS);
    // A larger configured timeout is clamped down to the ceiling, never up.
    cfg.request_timeout_ms = Some(20_000);
    assert_eq!(probe_timeout_secs(&cfg), PROBE_TIMEOUT_SECS);
    // Sub-second rounds up to a 1s floor.
    cfg.request_timeout_ms = Some(0);
    assert_eq!(probe_timeout_secs(&cfg), 1);
    cfg.request_timeout_ms = Some(500);
    assert_eq!(probe_timeout_secs(&cfg), 1);
    // A configured value below the ceiling shortens the probe.
    cfg.request_timeout_ms = Some(2_000);
    assert_eq!(probe_timeout_secs(&cfg), 2);
}

#[tokio::test]
#[serial]
async fn read_first_sse_json_skips_keepalive_and_returns_first_frame() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // A keepalive comment, then two data frames. The parser must skip the
    // comment (regression guard: it must not re-scan it forever) and return the
    // FIRST data frame, not the second.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/sse");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(": keepalive\n\ndata: {\"id\":1}\n\ndata: {\"id\":2}\n\n");
        })
        .await;

    let resp = probe_client()
        .get(server.url("/sse"))
        .send()
        .await
        .expect("sse get");
    let value = read_first_sse_json(resp, MAX_PROBE_BODY_BYTES)
        .await
        .expect("read ok")
        .expect("a parsed frame");
    assert_eq!(value.get("id").and_then(Value::as_i64), Some(1));
}

#[tokio::test]
#[serial]
async fn read_body_capped_truncates_at_cap() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    let big = "x".repeat(MAX_PROBE_BODY_BYTES + 4096);
    server
        .mock_async(|when, then| {
            when.method(GET).path("/big");
            then.status(200).body(&big);
        })
        .await;

    let resp = probe_client()
        .get(server.url("/big"))
        .send()
        .await
        .expect("get");
    let text = read_body_capped(resp, MAX_PROBE_BODY_BYTES)
        .await
        .expect("read ok");
    assert_eq!(text.len(), MAX_PROBE_BODY_BYTES);
}

#[tokio::test]
#[serial]
async fn non_json_post_falls_through_ladder_to_sse_get() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // Every JSON-RPC POST returns HTML — none of the POST probes match, so the
    // ladder must fall all the way through to the SSE GET transport probe.
    server
        .mock_async(|when, then| {
            when.method(POST).path("/x");
            then.status(200)
                .header("content-type", "text/html")
                .body("<!doctype html><html></html>");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/x");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("event: ping\ndata: {}\n\n");
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/x"))
        .await
        .expect("fall-through sse result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    assert_eq!(result.transport, Some(RpcTransport::Sse));
}

#[tokio::test]
#[serial]
async fn mcp_takes_precedence_over_openrpc() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // Server answers BOTH initialize (MCP) and rpc.discover (OpenRPC). MCP runs
    // first in the ladder, so the result must be MCP.
    server
        .mock_async(|when, then| {
            when.method(POST).path("/both").body_includes("initialize");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1,
                    "result": { "serverInfo": { "name": "both" }, "capabilities": {} }
                }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/both")
                .body_includes("rpc.discover");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 2,
                    "result": { "openrpc": "1.2.6", "methods": [] }
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/both"))
        .await
        .expect("precedence result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    assert_eq!(result.server_name.as_deref(), Some("both"));
}

#[tokio::test]
#[serial]
async fn mcp_sends_initialized_notification_with_session_id() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            // Match the quoted method name so this does not also swallow the
            // `notifications/initialized` POST (whose body contains the substring
            // "initialized").
            when.method(POST)
                .path("/mcp")
                .body_includes("\"initialize\"");
            then.status(200)
                .header("content-type", "application/json")
                .header("mcp-session-id", "sess-9")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1,
                    "result": { "serverInfo": { "name": "demo" }, "capabilities": {} }
                }));
        })
        .await;
    let initialized = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/mcp")
                .body_includes("notifications/initialized")
                .header("mcp-session-id", "sess-9");
            then.status(202);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("tools/list");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 2, "result": { "tools": [] }
                }));
        })
        .await;

    let result = probe_one(&probe_client(), &server.url("/mcp"))
        .await
        .expect("mcp result");
    assert_eq!(result.protocol, Some(RpcProtocol::Mcp));
    // The handshake notification must have been sent exactly once, carrying the
    // session id assigned by initialize.
    initialized.assert_calls_async(1).await;
}
