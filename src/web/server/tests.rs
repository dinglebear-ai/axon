//! Tests for `crates/web/server.rs` ask classification and ask route contracts.

#![allow(unsafe_code)]

use super::{ask_router, classify_ask_error};
use crate::mcp::auth::AuthPolicy;
use axum::http::StatusCode;
use serial_test::serial;
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug)]
struct Boom(String);
impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Boom {}

const ENV_KEY: &str = "AXON_MCP_HTTP_TOKEN";

struct EnvGuard {
    prev: Option<String>,
}

impl EnvGuard {
    fn set(value: Option<&str>) -> Self {
        let prev = std::env::var(ENV_KEY).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(ENV_KEY, v) },
            None => unsafe { std::env::remove_var(ENV_KEY) },
        }
        Self { prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => unsafe { std::env::set_var(ENV_KEY, v) },
            None => unsafe { std::env::remove_var(ENV_KEY) },
        }
    }
}

async fn spawn_ask_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let app = ask_router::<()>(
        Arc::new(crate::core::config::Config::default()),
        &auth_policy,
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("test server");
    });

    (format!("http://{addr}"), shutdown_tx, handle)
}

async fn stop(shutdown: oneshot::Sender<()>, handle: tokio::task::JoinHandle<()>) {
    let _ = shutdown.send(());
    handle.await.expect("server task");
}

#[test]
fn classify_bad_request() {
    let e = Boom("invalid query: empty".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(kind, "bad_request");
}

#[test]
fn classify_upstream() {
    let e = Boom("qdrant: connection refused".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn classify_upstream_timeout() {
    let e = Boom("TEI request timed out".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn classify_internal_default() {
    let e = Boom("something went sideways".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}

#[tokio::test]
#[serial]
async fn v1_ask_auth_layer_rejects_missing_and_wrong_tokens() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_ask_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "query": "" });

    let missing = client
        .post(format!("{base}/v1/ask"))
        .json(&body)
        .send()
        .await
        .expect("missing auth request");
    let wrong = client
        .post(format!("{base}/v1/ask"))
        .header("authorization", "Bearer wrong")
        .json(&body)
        .send()
        .await
        .expect("wrong auth request");

    stop(shutdown, handle).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn v1_ask_auth_layer_accepts_bearer_and_x_api_key() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_ask_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "query": "" });

    let bearer = client
        .post(format!("{base}/v1/ask"))
        .header("authorization", "Bearer secret")
        .json(&body)
        .send()
        .await
        .expect("bearer auth request");
    let api_key = client
        .post(format!("{base}/v1/ask"))
        .header("x-api-key", "secret")
        .json(&body)
        .send()
        .await
        .expect("x-api-key auth request");

    stop(shutdown, handle).await;
    assert_eq!(bearer.status(), StatusCode::BAD_REQUEST);
    assert_eq!(api_key.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn v1_ask_accepts_graph_false_and_rejects_graph_true_before_query_validation() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_ask_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let graph_false = client
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "", "graph": false }))
        .send()
        .await
        .expect("graph false request");
    let graph_false_status = graph_false.status();
    let graph_false_body: serde_json::Value = graph_false.json().await.expect("graph false body");
    let graph_true = client
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "", "graph": true }))
        .send()
        .await
        .expect("graph true request");
    let graph_true_status = graph_true.status();
    let graph_true_body: serde_json::Value = graph_true.json().await.expect("graph true body");

    stop(shutdown, handle).await;
    assert_eq!(graph_false_status, StatusCode::BAD_REQUEST);
    assert_eq!(graph_false_body["message"], "query is required");
    assert_eq!(graph_true_status, StatusCode::BAD_REQUEST);
    assert_eq!(
        graph_true_body["message"],
        "graph retrieval is not supported; omit graph or set graph to false"
    );
}
