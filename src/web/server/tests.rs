//! Tests for `crates/web/server.rs` ask classification and ask route contracts.

#![allow(unsafe_code)]

use super::{HttpError, ScopeRequirement, ask_router, protect_routes};
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::mcp::auth::AuthPolicy;
use crate::services::context::ServiceContext;
use crate::services::runtime::ServiceJobRuntime;
use crate::services::types::ServiceJob;
use async_trait::async_trait;
use axum::http::StatusCode;
use serial_test::serial;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug)]
struct Boom(String);
impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for Boom {}

const ENV_KEY: &str = "AXON_MCP_HTTP_TOKEN";

struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(value: Option<&str>) -> Self {
        Self::set_key(ENV_KEY, value)
    }

    fn set_key(key: &'static str, value: Option<&str>) -> Self {
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

struct EmptyRuntime;

#[async_trait]
impl ServiceJobRuntime for EmptyRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        Err("not implemented".into())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Err("not implemented".into())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }
}

async fn spawn_ask_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let app = protect_routes(
        ask_router::<()>(Arc::new(crate::core::config::Config::default())),
        &auth_policy,
        ScopeRequirement::Write,
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("test server");
    });

    (format!("http://{addr}"), shutdown_tx, handle)
}

async fn spawn_full_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let home = tempfile::tempdir().expect("temp home");
    let _home_guard = EnvGuard::set_key("HOME", home.path().to_str());
    let panel = Arc::new(super::PanelRuntimeState::initialize("127.0.0.1", 0).expect("panel"));
    let cfg = Arc::new(crate::core::config::Config::default());
    let ctx = Arc::new(ServiceContext::from_runtime(
        Arc::clone(&cfg),
        Arc::new(EmptyRuntime),
    ));
    let app = super::router(cfg, panel, ctx, auth_policy);
    drop(_home_guard);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("test server");
        drop(home);
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
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.kind(), "bad_request");
}

#[test]
fn classify_upstream() {
    let e = Boom("qdrant: connection refused".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(err.kind(), "upstream_unavailable");
}

#[test]
fn classify_upstream_timeout() {
    let e = Boom("TEI request timed out".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(err.kind(), "timeout");
}

#[test]
fn classify_internal_default() {
    let e = Boom("something went sideways".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.kind(), "internal");
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
async fn all_v1_rest_routes_reject_missing_auth_when_auth_is_configured() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let job_id = Uuid::nil();
    let crawl_job = format!("/v1/crawl/{job_id}");
    let crawl_cancel = format!("/v1/crawl/{job_id}/cancel");
    let embed_job = format!("/v1/embed/{job_id}");
    let extract_job = format!("/v1/extract/{job_id}");
    let ingest_job = format!("/v1/ingest/{job_id}");
    let watch_run = format!("/v1/watch/{job_id}/run");
    let routes = [
        ("GET", "/v1/capabilities"),
        ("POST", "/v1/actions"),
        ("GET", "/v1/sources"),
        ("GET", "/v1/domains"),
        ("GET", "/v1/stats"),
        ("GET", "/v1/status"),
        ("GET", "/v1/doctor"),
        ("POST", "/v1/ask"),
        ("POST", "/v1/query"),
        ("POST", "/v1/retrieve"),
        ("POST", "/v1/evaluate"),
        ("POST", "/v1/suggest"),
        ("POST", "/v1/scrape"),
        ("POST", "/v1/map"),
        ("POST", "/v1/search"),
        ("POST", "/v1/research"),
        ("POST", "/v1/crawl"),
        ("GET", crawl_job.as_str()),
        ("POST", crawl_cancel.as_str()),
        ("POST", "/v1/crawl/cleanup"),
        ("DELETE", "/v1/crawl"),
        ("POST", "/v1/crawl/recover"),
        ("POST", "/v1/embed"),
        ("GET", embed_job.as_str()),
        ("POST", "/v1/extract"),
        ("GET", extract_job.as_str()),
        ("POST", "/v1/ingest"),
        ("GET", ingest_job.as_str()),
        ("POST", "/v1/dedupe"),
        ("GET", "/v1/watch"),
        ("POST", "/v1/watch"),
        ("POST", watch_run.as_str()),
    ];

    for (method, path) in routes {
        let response = match method {
            "DELETE" => client.delete(format!("{base}{path}")).send().await,
            "GET" => client.get(format!("{base}{path}")).send().await,
            "POST" => {
                client
                    .post(format!("{base}{path}"))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
            }
            _ => unreachable!("unexpected test method"),
        }
        .unwrap_or_else(|err| panic!("{method} {path} failed: {err}"));
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} should reject missing auth"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn loopback_dev_blocks_destructive_rest_routes_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let job_id = Uuid::nil();
    let watch_run = format!("/v1/watch/{job_id}/run");
    let crawl_cancel = format!("/v1/crawl/{job_id}/cancel");
    let routes = [
        ("POST", "/v1/dedupe"),
        ("POST", "/v1/watch"),
        ("POST", watch_run.as_str()),
        ("POST", "/v1/crawl"),
        ("POST", crawl_cancel.as_str()),
        ("POST", "/v1/crawl/cleanup"),
        ("DELETE", "/v1/crawl"),
        ("POST", "/v1/crawl/recover"),
    ];

    for (method, path) in routes {
        let response = match method {
            "DELETE" => client.delete(format!("{base}{path}")).send().await,
            "POST" => {
                client
                    .post(format!("{base}{path}"))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
            }
            _ => unreachable!("unexpected test method"),
        }
        .unwrap_or_else(|err| panic!("{method} {path} failed: {err}"));
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} should reject missing auth in loopback dev"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn loopback_dev_allows_non_destructive_write_routes_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "" }))
        .send()
        .await
        .expect("ask request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
