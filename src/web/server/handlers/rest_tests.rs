#![allow(unsafe_code)]
//! Family 1 (read-only GET routes) wiring + auth tests.
//!
//! These tests boot a minimal server with `EmptyRuntime` and exercise the
//! scope guard / route mounting. Service calls that need Qdrant return
//! 502/upstream — the assertions stay on the routing and auth layer, not on
//! the payload body, so the tests run without live infra.

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

async fn spawn(
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
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .expect("serve");
    });
    (format!("http://{addr}"), tx, handle)
}

async fn stop(tx: oneshot::Sender<()>, handle: tokio::task::JoinHandle<()>) {
    let _ = tx.send(());
    handle.await.expect("join");
}

/// In LoopbackDev mode every GET route is reachable without a token (status
/// may be 200 or 502 depending on Qdrant availability, but never 404/401).
#[tokio::test]
#[serial]
async fn loopback_dev_read_routes_are_reachable_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    for path in [
        "/v1/sources",
        "/v1/domains",
        "/v1/stats",
        "/v1/doctor",
        "/v1/status",
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        assert!(
            status != StatusCode::NOT_FOUND && status != StatusCode::UNAUTHORIZED,
            "route {path} unexpectedly returned {status}"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn bearer_only_read_routes_require_auth() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/v1/sources"))
        .send()
        .await
        .expect("missing-auth request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["kind"], "unauthorized");
}

/// F2 sync POST routes return 400 when the required string field is empty
/// or missing (LoopbackDev avoids the auth layer so we exercise the body
/// validation path only).
#[tokio::test]
#[serial]
async fn sync_post_routes_reject_empty_required_fields() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    // (path, body) — each body omits/empties the required field.
    let cases = [
        ("/v1/query", serde_json::json!({ "query": "" })),
        ("/v1/retrieve", serde_json::json!({ "url": "" })),
        ("/v1/map", serde_json::json!({ "url": "" })),
        ("/v1/search", serde_json::json!({ "query": "  " })),
        ("/v1/research", serde_json::json!({ "query": "" })),
        ("/v1/scrape", serde_json::json!({ "url": "" })),
    ];

    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        assert_eq!(body["kind"], "bad_request", "{path} kind");
    }

    stop(shutdown, handle).await;
}

/// F2 search/research time_range parsing rejects invalid values.
#[tokio::test]
#[serial]
async fn sync_post_search_rejects_invalid_time_range() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/search"))
        .json(&serde_json::json!({ "query": "test", "time_range": "decade" }))
        .send()
        .await
        .expect("request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("time_range"),
        "expected time_range error, got {body}"
    );
}

/// F3 async-job submit routes reject empty/missing required fields with 400
/// in LoopbackDev mode (no auth layer involved).
#[tokio::test]
#[serial]
async fn async_submit_routes_reject_empty_required_fields() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let cases = [
        ("/v1/crawl", serde_json::json!({ "urls": [] })),
        ("/v1/embed", serde_json::json!({ "input": "" })),
        ("/v1/extract", serde_json::json!({ "urls": [] })),
    ];
    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        assert_eq!(body["kind"], "bad_request", "{path} kind");
    }

    stop(shutdown, handle).await;
}

/// F3 GET / cancel routes reject non-UUID :id with 400.
#[tokio::test]
#[serial]
async fn async_job_id_routes_reject_invalid_uuid() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    for path in [
        "/v1/crawl/not-a-uuid",
        "/v1/embed/not-a-uuid",
        "/v1/extract/not-a-uuid",
        "/v1/ingest/not-a-uuid",
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("get {path}: {e}"));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }

    for path in [
        "/v1/crawl/not-a-uuid/cancel",
        "/v1/embed/not-a-uuid/cancel",
        "/v1/extract/not-a-uuid/cancel",
        "/v1/ingest/not-a-uuid/cancel",
    ] {
        let response = client
            .post(format!("{base}{path}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("post {path}: {e}"));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }

    stop(shutdown, handle).await;
}

/// F3 review-followup: valid-UUID-but-unknown-job returns 404 (not 200 with
/// a null payload). Specifically guards the crawl path which uses a service
/// that returns `Result<CrawlJobResult>` rather than `Result<Option<_>>`.
#[tokio::test]
#[serial]
async fn async_status_returns_404_for_unknown_job() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let unknown = "00000000-0000-0000-0000-000000000000";

    for kind in ["crawl", "embed", "extract", "ingest"] {
        let response = client
            .get(format!("{base}/v1/{kind}/{unknown}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("get {kind}/{unknown}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind} expected 404");
        assert_eq!(body["kind"], "not_found", "{kind} kind");
    }

    stop(shutdown, handle).await;
}

/// Review-followup: deny_unknown_fields on body structs actually rejects
/// unknown fields with 400 (axum surfaces serde_json::Deserialize errors as
/// 422 by default, but the resulting JSON body should still carry an error).
#[tokio::test]
#[serial]
async fn sync_post_rejects_unknown_fields() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/query"))
        .json(&serde_json::json!({
            "query": "test",
            "definitely_not_a_field": 1
        }))
        .send()
        .await
        .expect("query with bogus field");
    let status = response.status();

    stop(shutdown, handle).await;
    // Axum's JsonRejection emits 4xx for unknown fields via the
    // deny_unknown_fields contract — either 400 or 422 is acceptable; the
    // hard requirement is that it does NOT reach the handler with the bogus
    // field silently accepted (which would be 200/4xx-from-service).
    assert!(
        status.is_client_error() && status != StatusCode::NOT_FOUND,
        "expected 4xx client error, got {status}"
    );
}

/// Review-followup: scope discrimination. A token with only `axon:read`
/// scope passes read-scope routes but is rejected on write-scope routes
/// (e.g. /v1/scrape) with 403.
///
/// Implementation note: in bearer-only mode the static AXON_MCP_HTTP_TOKEN
/// is granted BOTH axon:read AND axon:write (see mcp::auth::build_auth_layer);
/// to exercise the discrimination path against only read scope we would need
/// an OAuth token, which the test harness does not currently provision.
/// Instead, this test documents the contract by exercising the inverse:
/// a valid bearer token (which has axon:write) successfully reaches a
/// write-scope route, confirming the scope guard does not block valid
/// write tokens.
#[tokio::test]
#[serial]
async fn bearer_token_passes_write_scope_guard() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/scrape"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "url": "https://example.invalid/" }))
        .send()
        .await
        .expect("scrape request");
    let status = response.status();

    stop(shutdown, handle).await;
    // 200 (Qdrant/Chrome reachable, surprising), 502 (upstream not running),
    // or 400 from URL validation — none of these are 401/403 which would mean
    // the scope guard incorrectly blocked the valid write token.
    assert_ne!(status, StatusCode::UNAUTHORIZED, "valid bearer rejected");
    assert_ne!(status, StatusCode::FORBIDDEN, "valid bearer rejected");
}

/// Review-followup: positive auth test for admin routes. With a valid bearer
/// token (axon:write scope) the migrate route passes the admin_write guard
/// and reaches body validation. Empty `from` then returns 400 from the
/// handler — proving the request crossed the auth boundary.
#[tokio::test]
#[serial]
async fn admin_routes_accept_valid_bearer() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/migrate"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "from": "", "to": "dst" }))
        .send()
        .await
        .expect("migrate request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "should reach handler, got {status}"
    );
    assert_eq!(body["kind"], "bad_request");
    assert!(
        body["message"].as_str().unwrap_or("").contains("from"),
        "should be from-field validation, got {body}"
    );
}

/// F4: POST /v1/migrate and /v1/dedupe require auth EVEN in LoopbackDev
/// (admin_write guard). Mirrors the existing /v1/actions Migrate/Dedupe
/// invariant in src/web/actions.rs.
#[tokio::test]
#[serial]
async fn admin_routes_require_auth_in_loopback_dev() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let migrate = client
        .post(format!("{base}/v1/migrate"))
        .json(&serde_json::json!({ "from": "src", "to": "dst" }))
        .send()
        .await
        .expect("migrate request");
    let migrate_status = migrate.status();

    let dedupe = client
        .post(format!("{base}/v1/dedupe"))
        .send()
        .await
        .expect("dedupe request");
    let dedupe_status = dedupe.status();

    stop(shutdown, handle).await;
    assert_eq!(
        migrate_status,
        StatusCode::UNAUTHORIZED,
        "migrate must require auth in LoopbackDev"
    );
    assert_eq!(
        dedupe_status,
        StatusCode::UNAUTHORIZED,
        "dedupe must require auth in LoopbackDev"
    );
}

/// F4: migrate rejects empty from/to with 400 when authenticated.
#[tokio::test]
#[serial]
async fn migrate_rejects_empty_fields_when_authed() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/migrate"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "from": "", "to": "dst" }))
        .send()
        .await
        .expect("migrate request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "bad_request");
    assert!(body["message"].as_str().unwrap_or("").contains("from"));
}

#[tokio::test]
#[serial]
async fn bearer_token_grants_read_access() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/v1/doctor"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("authed request");
    let status = response.status();

    stop(shutdown, handle).await;
    // Either 200 (Qdrant reachable in CI sandbox) or 502 (upstream not running)
    // — both prove the auth+scope guard let the request through.
    assert!(
        status == StatusCode::OK || status == StatusCode::BAD_GATEWAY,
        "expected 200 or 502 with token, got {status}"
    );
}
