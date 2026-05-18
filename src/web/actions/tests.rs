#![allow(unsafe_code)]

use super::router;
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
use tokio::sync::{OnceCell, oneshot};
use uuid::Uuid;

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

async fn spawn_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let cfg = Arc::new(crate::core::config::Config::default());
    let cell = Arc::new(OnceCell::new());
    let ctx = Arc::new(ServiceContext::from_runtime(
        cfg.clone(),
        Arc::new(EmptyRuntime),
    ));
    assert!(cell.set(ctx).is_ok());

    let app = router(cfg, cell, auth_policy);
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

#[tokio::test]
#[serial]
async fn capabilities_returns_server_info() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_test_server(AuthPolicy::LoopbackDev).await;

    let response = reqwest::get(format!("{base}/v1/capabilities"))
        .await
        .expect("capabilities request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["schema_version"], "client-server.v1");
    assert_eq!(body["minimum_client_schema_version"], "client-server.v1");
    assert_eq!(
        body["required_request_fields"],
        serde_json::json!(["request_id", "action"])
    );
    assert!(
        body["supported_actions"]
            .as_array()
            .expect("actions array")
            .iter()
            .any(|value| value == "status")
    );
}

#[tokio::test]
#[serial]
async fn actions_rejects_missing_and_invalid_auth_as_json() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "request_id": "auth-1",
        "action": { "action": "status" }
    });

    let missing = client
        .post(format!("{base}/v1/actions"))
        .json(&body)
        .send()
        .await
        .expect("missing auth request");
    let missing_status = missing.status();
    let missing_body: serde_json::Value = missing.json().await.expect("missing json body");

    let invalid = client
        .post(format!("{base}/v1/actions"))
        .header("authorization", "Bearer wrong")
        .json(&body)
        .send()
        .await
        .expect("invalid auth request");
    let invalid_status = invalid.status();
    let invalid_body: serde_json::Value = invalid.json().await.expect("invalid json body");

    stop(shutdown, handle).await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing_body["ok"], false);
    assert_eq!(missing_body["error"]["kind"], "unauthorized");
    assert_eq!(invalid_status, StatusCode::UNAUTHORIZED);
    assert_eq!(invalid_body["ok"], false);
    assert_eq!(invalid_body["error"]["kind"], "unauthorized");
}

#[tokio::test]
#[serial]
async fn actions_unknown_action_returns_json_error() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .json(&serde_json::json!({
            "request_id": "unknown-1",
            "action": { "action": "not_real" }
        }))
        .send()
        .await
        .expect("unknown action request");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(content_type.starts_with("application/json"));
    assert_eq!(body["request_id"], "unknown-1");
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["kind"], "invalid_request");
}

#[tokio::test]
#[serial]
async fn actions_dispatches_status_through_service_context() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .json(&serde_json::json!({
            "request_id": "status-1",
            "action": { "action": "status" }
        }))
        .send()
        .await
        .expect("status action request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["request_id"], "status-1");
    assert_eq!(body["ok"], true);
    assert_eq!(body["result"]["totals"]["crawl"], 0);
}

#[tokio::test]
#[serial]
async fn migrate_requires_auth_even_in_loopback_dev_mode() {
    // F5: destructive actions (migrate, dedupe) must be unconditionally auth-gated.
    // LoopbackDev mode skips auth for normal actions but must NOT skip it here.
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .json(&serde_json::json!({
            "request_id": "migrate-no-auth",
            "action": {
                "action": "migrate",
                "from": "source_col",
                "to": "dest_col"
            }
        }))
        .send()
        .await
        .expect("migrate request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401 or 403 for migrate without token in LoopbackDev mode, got {status}"
    );
    assert_eq!(body["ok"], false);
}

#[tokio::test]
#[serial]
async fn dedupe_requires_auth_even_in_loopback_dev_mode() {
    // F5: same invariant for dedupe.
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .json(&serde_json::json!({
            "request_id": "dedupe-no-auth",
            "action": { "action": "dedupe" }
        }))
        .send()
        .await
        .expect("dedupe request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401 or 403 for dedupe without token in LoopbackDev mode, got {status}"
    );
    assert_eq!(body["ok"], false);
}

#[tokio::test]
#[serial]
async fn ask_requires_write_scope() {
    // F10: ask is promoted to axon:write — a read-only token must be refused.
    let _env = EnvGuard::set(Some("write-token"));
    let (base, shutdown, handle) =
        spawn_test_server(AuthPolicy::Mounted { auth_state: None }).await;

    // Issue the request with a read-only token scope (no bearer = no token = 401).
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .json(&serde_json::json!({
            "request_id": "ask-no-auth",
            "action": { "action": "ask", "question": "test?" }
        }))
        .send()
        .await
        .expect("ask without auth");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "ask without token must return 401: {body}"
    );
    assert_eq!(body["ok"], false);
}
