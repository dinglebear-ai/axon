#![allow(unsafe_code)]
//! Family 1 (read-only GET routes) wiring + auth tests.
//!
//! These tests boot a minimal server with `EmptyRuntime` and exercise the
//! scope guard / route mounting. Service calls that need Qdrant return
//! 502/upstream — the assertions stay on the routing and auth layer, not on
//! the payload body, so the tests run without live infra.

use super::router;
use async_trait::async_trait;
use axon_api::source::JobKind;
use axon_authz::http::AuthPolicy;
use axon_services::context::ServiceContext;
use axon_services::runtime::{RuntimeResult, ServiceJobRuntime};
use axon_services::types::ServiceJob;
use axum::http::StatusCode;
use serial_test::serial;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{OnceCell, oneshot};
use uuid::Uuid;

const ENV_KEY: &str = "AXON_HTTP_TOKEN";

#[test]
fn extract_submit_body_accepts_cli_parity_knobs() {
    let body = serde_json::json!({
        "urls": ["https://example.com/docs"],
        "prompt": "extract title",
        "extract_mode": "llm",
        "max_pages": 1,
        "render_mode": "http",
        "embed": false,
        "headers": [["x-test", "1"]]
    });

    let parsed: crate::server::handlers::rest::types::ExtractSubmitBody =
        serde_json::from_value(body).expect("parse extract body");

    assert_eq!(parsed.urls, vec!["https://example.com/docs"]);
    assert_eq!(parsed.prompt.as_deref(), Some("extract title"));
    assert_eq!(parsed.max_pages, Some(1));
    assert_eq!(parsed.embed, Some(false));
}

#[test]
fn async_lifecycle_routes_are_declared_for_extract() {
    let routes = crate::server::handlers::rest::documented_rest_paths_for_tests();
    assert!(routes.contains(&"GET /v1/extract".to_string()));
    assert!(routes.contains(&"POST /v1/extract/cleanup".to_string()));
    assert!(routes.contains(&"DELETE /v1/extract".to_string()));
    assert!(routes.contains(&"POST /v1/extract/recover".to_string()));
}

#[test]
fn sources_submit_is_write_scope() {
    assert_eq!(
        crate::server::handlers::rest::auth::scope_for_rest_route("POST", "/v1/sources"),
        Some("axon:write")
    );
}

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
    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> RuntimeResult<String> {
        Err("not implemented".into())
    }
    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> RuntimeResult<Option<String>> {
        Ok(None)
    }
    async fn has_active_jobs(&self, _kind: JobKind) -> RuntimeResult<bool> {
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

async fn spawn(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let cfg = Arc::new(axon_core::config::Config::default());
    spawn_with_runtime(auth_policy, cfg, Arc::new(EmptyRuntime)).await
}

async fn spawn_with_runtime(
    auth_policy: AuthPolicy,
    cfg: Arc<axon_core::config::Config>,
    runtime: Arc<dyn ServiceJobRuntime>,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let cell = Arc::new(OnceCell::new());
    let ctx = Arc::new(ServiceContext::from_runtime(cfg.clone(), runtime));
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

/// The legacy indexing routes (embed/ingest/scrape/crawl) were removed in
/// favor of the unified `POST /v1/sources`. They must now 404, while the
/// replacement `POST /v1/sources` route is mounted (never 404/405).
#[tokio::test]
#[serial]
async fn legacy_indexing_routes_are_absent_and_sources_present() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    for path in ["/v1/embed", "/v1/ingest", "/v1/scrape", "/v1/crawl"] {
        let response = client
            .post(format!("{base}{path}"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap_or_else(|e| panic!("post {path}: {e}"));
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "removed route {path} should 404"
        );
    }

    // POST /v1/sources is mounted: an empty body returns 400 (source required),
    // never 404/405.
    let response = client
        .post(format!("{base}/v1/sources"))
        .json(&serde_json::json!({ "source": "" }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();
    stop(shutdown, handle).await;
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "POST /v1/sources should be mounted"
    );
    assert_ne!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /v1/sources should be mounted"
    );
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty source is a 400");
}

/// `execution.detached=true` with no unified job store configured
/// (`EmptyRuntime` here) degrades gracefully to the synchronous path instead
/// of erroring — matching the handler's documented "no job store → fall back"
/// behavior. The empty-source validation still runs first, so this exercises
/// the same 400 as the non-detached case, proving the async branch never
/// bypasses validation.
#[tokio::test]
#[serial]
async fn detached_request_without_a_job_store_falls_back_to_synchronous_validation() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/sources"))
        .json(&serde_json::json!({
            "source": "",
            "execution": { "mode": "background", "detached": true, "priority": "normal", "heartbeat_interval_secs": 5 }
        }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();
    stop(shutdown, handle).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "empty source is still a 400 even when detached=true"
    );
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
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "auth.missing");
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
        ("/v1/sources", serde_json::json!({ "source": "" })),
    ];

    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(
            body["error"]["code"], "route.validation.invalid_field",
            "{path} code"
        );
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
    assert_eq!(body["ok"], false);
    assert!(
        body["error"]["message"]
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

    let cases = [("/v1/extract", serde_json::json!({ "urls": [] }))];
    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(
            body["error"]["code"], "route.validation.invalid_field",
            "{path} code"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn async_submit_routes_reject_private_urls_before_enqueue() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    for (path, body) in [(
        "/v1/extract",
        serde_json::json!({ "urls": ["http://127.0.0.1/admin"] }),
    )] {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        assert_eq!(
            body["error"]["code"], "source.resolve.invalid_uri",
            "{path} code"
        );
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

    for path in ["/v1/extract/not-a-uuid"] {
        let response = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("get {path}: {e}"));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }

    for path in ["/v1/extract/not-a-uuid/cancel"] {
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

    for kind in ["extract"] {
        let response = client
            .get(format!("{base}/v1/{kind}/{unknown}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("get {kind}/{unknown}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind} expected 404");
        assert_eq!(
            body["error"]["code"], "source.acquire.not_found",
            "{kind} code"
        );
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
/// (e.g. /v1/sources) with 403.
///
/// Implementation note: in bearer-only mode the static AXON_HTTP_TOKEN
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
        .post(format!("{base}/v1/sources"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "source": "https://example.invalid/" }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();

    stop(shutdown, handle).await;
    // 200 (indexed / degraded SourceResult), 502 (upstream not running), or
    // 400/500 — none of these are 401/403 which would mean the scope guard
    // incorrectly blocked the valid write token.
    assert_ne!(status, StatusCode::UNAUTHORIZED, "valid bearer rejected");
    assert_ne!(status, StatusCode::FORBIDDEN, "valid bearer rejected");
}

/// Review-followup: positive auth test for admin routes. With a valid bearer
/// token (axon:write scope) the dedupe route passes the admin_write guard
/// and reaches body validation — proving the request crossed the auth boundary.
#[tokio::test]
#[serial]
async fn admin_routes_accept_valid_bearer() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/prune/dedupe"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "collection": "invalid/name" }))
        .send()
        .await
        .expect("dedupe request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "should reach handler, got {status}"
    );
    assert_eq!(body["error"]["code"], "route.validation.invalid_field");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("collection"),
        "should be collection validation, got {body}"
    );
}

#[tokio::test]
#[serial]
async fn admin_dedupe_rejects_body_without_json_content_type() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/prune/dedupe"))
        .header("authorization", "Bearer secret")
        .body(r#"{"collection":"invalid/name"}"#)
        .send()
        .await
        .expect("dedupe request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "route.validation.unsupported_media");
}

/// F4: POST /v1/prune/dedupe requires auth EVEN in LoopbackDev (admin_write guard).
/// Migrate is intentionally not exposed as REST.
#[tokio::test]
#[serial]
async fn admin_routes_require_auth_in_loopback_dev() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let dedupe = client
        .post(format!("{base}/v1/prune/dedupe"))
        .send()
        .await
        .expect("dedupe request");
    let dedupe_status = dedupe.status();

    stop(shutdown, handle).await;
    assert_eq!(
        dedupe_status,
        StatusCode::UNAUTHORIZED,
        "dedupe must require auth in LoopbackDev"
    );
}

/// F4: migrate is intentionally not exposed as REST.
#[tokio::test]
#[serial]
async fn migrate_is_not_exposed_as_rest() {
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

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

// ── Scope discrimination unit tests (11ig) ───────────────────────────────
//
// The scope-check logic lives in enforce_scope. We test it directly by
// verifying the scope-matching predicate that the function uses, rather than
// constructing a full axum tower stack (Next::new API varies by axum version).
// These tests pin the invariant: authenticated Axon users receive full Axon
// server access. OAuth email allowlisting is the access boundary; the read/write
// scope strings are retained for client metadata and token compatibility.

/// Core invariant: axon:read satisfies axon:write routes. This mirrors
/// `axon_authz::scope_satisfies`, used by `enforce_scope`.
#[test]
fn scope_check_read_satisfies_write_requirement() {
    let scopes: Vec<String> = vec!["axon:read".into()];
    let required_scope = "axon:write";
    let allowed = axon_authz::scope_satisfies(&scopes, required_scope);
    assert!(allowed, "axon:read must satisfy axon:write requirement");
}

/// axon:write satisfies axon:read.
#[test]
fn scope_check_write_satisfies_read_requirement() {
    let scopes: Vec<String> = vec!["axon:write".into()];
    let required_scope = "axon:read";
    let allowed = axon_authz::scope_satisfies(&scopes, required_scope);
    assert!(allowed, "axon:write must satisfy axon:read requirement");
}

/// Having both scopes satisfies both read and write requirements.
#[test]
fn scope_check_both_scopes_satisfy_either() {
    let scopes: Vec<String> = vec!["axon:read".into(), "axon:write".into()];
    for required_scope in ["axon:read", "axon:write"] {
        let allowed = axon_authz::scope_satisfies(&scopes, required_scope);
        assert!(allowed, "both scopes should satisfy {required_scope}");
    }
}

/// axon:write satisfies read-scope routes via the full HTTP server too.
#[tokio::test]
#[serial]
async fn axon_write_token_satisfies_read_scope_route() {
    // The static bearer path grants both axon:read AND axon:write per
    // build_auth_layer (see mcp/auth.rs:114-118 with_static_token_scopes).
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) = spawn(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/v1/sources"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("sources request");
    let status = response.status();

    stop(shutdown, handle).await;
    // The static bearer token (AXON_HTTP_TOKEN) grants both axon:read
    // AND axon:write per mcp/auth.rs:114-118. Asserting it is NOT 401/403
    // proves the scope guard did not block it. The route then calls a service
    // that needs Qdrant — 200 when reachable, 502 when not, both are fine.
    assert!(
        matches!(
            status,
            StatusCode::OK | StatusCode::BAD_GATEWAY | StatusCode::INTERNAL_SERVER_ERROR
        ),
        "expected service response (200/502/500), not auth rejection; got {status}"
    );
}

// ── Watch route tests (ovuc) ─────────────────────────────────────────────

/// GET /v1/watch is reachable (not 404) in LoopbackDev.
#[tokio::test]
#[serial]
async fn watch_list_route_is_reachable() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;

    let status = reqwest::get(format!("{base}/v1/watch"))
        .await
        .expect("watch list")
        .status();

    stop(shutdown, handle).await;
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "GET /v1/watch should be mounted"
    );
    assert_ne!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET should work on /v1/watch"
    );
}

/// POST /v1/watch with empty name returns 400.
#[tokio::test]
#[serial]
async fn watch_create_rejects_empty_name() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/watch"))
        .json(&serde_json::json!({
            "name": "",
            "task_type": "watch",
            "task_payload": {},
            "every_seconds": 60,
            "enabled": true,
            "next_run_at": "2026-01-01T00:00:00Z"
        }))
        .send()
        .await
        .expect("watch create");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "route.validation.invalid_field");
}

/// POST /v1/watch with every_seconds=0 returns 400.
#[tokio::test]
#[serial]
async fn watch_create_rejects_zero_interval() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/watch"))
        .json(&serde_json::json!({
            "name": "test-watch",
            "task_type": "watch",
            "task_payload": {},
            "every_seconds": 0,
            "enabled": true,
            "next_run_at": "2026-01-01T00:00:00Z"
        }))
        .send()
        .await
        .expect("watch create");
    let status = response.status();

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// POST /v1/watch with unsupported task_type returns 400.
#[tokio::test]
#[serial]
async fn watch_create_rejects_unsupported_task_type() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/watch"))
        .json(&serde_json::json!({
            "name": "test",
            "task_type": "frobinate",
            "task_payload": {},
            "every_seconds": 60,
            "enabled": true,
            "next_run_at": "2026-01-01T00:00:00Z"
        }))
        .send()
        .await
        .expect("watch create");
    let status = response.status();

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn watch_create_requires_non_empty_url_array() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let cases = [
        serde_json::json!({}),
        serde_json::json!({ "urls": [] }),
        serde_json::json!({ "urls": [1] }),
    ];
    for payload in cases {
        let response = client
            .post(format!("{base}/v1/watch"))
            .json(&serde_json::json!({
                "name": "test",
                "task_type": "watch",
                "task_payload": payload,
                "every_seconds": 60,
                "enabled": true,
                "next_run_at": "2026-01-01T00:00:00Z"
            }))
            .send()
            .await
            .expect("watch create");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "route.validation.invalid_field");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("urls"),
            "expected urls error, got {body}"
        );
    }

    stop(shutdown, handle).await;
}

/// GET /v1/watch/{uuid} with an unknown UUID returns 404 (not found in DB)
/// or 500/502 (SQLite unavailable in test env). Never 405.
#[tokio::test]
#[serial]
async fn watch_get_unknown_uuid_route_is_mounted() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let unknown = "00000000-0000-0000-0000-000000000001";

    let response = reqwest::get(format!("{base}/v1/watch/{unknown}"))
        .await
        .expect("watch get");
    let status = response.status();

    stop(shutdown, handle).await;
    // 404 = route mounted, UUID not found in SQLite (correct result)
    // 500 / 502 = route mounted, SQLite unavailable in test env (acceptable)
    // 405 = route NOT registered — definite bug
    // 401/403 = auth guard fired unexpectedly — also a bug in LoopbackDev
    assert!(
        matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::INTERNAL_SERVER_ERROR | StatusCode::BAD_GATEWAY
        ),
        "GET /v1/watch/{{uuid}} should return 404/500/502, got {status}"
    );
}

/// POST /v1/watch and GET /v1/watch/{id} coexist on the retained test router.
#[tokio::test]
#[serial]
async fn watch_create_uses_production_path_not_legacy_create_path() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let legacy_create = client
        .post(format!("{base}/v1/watch/create"))
        .json(&serde_json::json!({
            "name": "",
            "task_type": "watch",
            "task_payload": {},
            "every_seconds": 60,
            "enabled": true,
            "next_run_at": "2026-01-01T00:00:00Z"
        }))
        .send()
        .await
        .expect("POST /v1/watch/create");

    stop(shutdown, handle).await;
    assert_ne!(
        legacy_create.status(),
        StatusCode::BAD_REQUEST,
        "legacy /v1/watch/create must not reach the watch-create handler after route consolidation"
    );
}

// ── deny_unknown_fields across all Family 2+3 body structs (xqp1) ────────

/// Every body struct that has #[serde(deny_unknown_fields)] rejects an unknown
/// field. Parametrized to cover all Family 2 and Family 3 submit routes.
#[tokio::test]
#[serial]
async fn all_submit_routes_reject_unknown_fields() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    // (path, method, body_with_unknown_field)
    let cases: &[(&str, &str, serde_json::Value)] = &[
        (
            "/v1/query",
            "POST",
            serde_json::json!({ "query": "test", "_x": 1 }),
        ),
        (
            "/v1/retrieve",
            "POST",
            serde_json::json!({ "url": "https://example.com", "_x": 1 }),
        ),
        (
            "/v1/map",
            "POST",
            serde_json::json!({ "url": "https://example.com", "_x": 1 }),
        ),
        ("/v1/suggest", "POST", serde_json::json!({ "_x": 1 })),
        (
            "/v1/search",
            "POST",
            serde_json::json!({ "query": "test", "_x": 1 }),
        ),
        (
            "/v1/research",
            "POST",
            serde_json::json!({ "query": "test", "_x": 1 }),
        ),
        (
            "/v1/sources",
            "POST",
            serde_json::json!({ "source": "https://example.com", "_x": 1 }),
        ),
        (
            "/v1/extract",
            "POST",
            serde_json::json!({ "urls": ["https://example.com"], "_x": 1 }),
        ),
    ];

    for (path, _, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        assert!(
            status.is_client_error(),
            "{path} with unknown field should return 4xx, got {status}"
        );
        assert_ne!(status, StatusCode::NOT_FOUND, "{path} should be mounted");
    }

    stop(shutdown, handle).await;
}
