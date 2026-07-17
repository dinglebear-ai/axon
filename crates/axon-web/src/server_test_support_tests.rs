#![allow(unsafe_code)]

use super::{ScopeRequirement, ask_router, protect_routes};
use async_trait::async_trait;
use axon_api::source::{ArtifactKind, JobKind, MetadataMap};
use axon_authz::http::AuthPolicy;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use axon_services::context::ServiceContext;
use axon_services::runtime::{RuntimeResult, ServiceJobRuntime};
use axon_services::types::ServiceJob;
use axum::http::{StatusCode, header};
use serial_test::serial;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

const ENV_KEY: &str = "AXON_HTTP_TOKEN";

pub(super) struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    pub(super) fn set(value: Option<&str>) -> Self {
        Self::set_key(ENV_KEY, value)
    }

    pub(super) fn set_key(key: &'static str, value: Option<&str>) -> Self {
        let prev = std::env::var(key).ok();
        match value {
            // SAFETY: EnvGuard is test-only. Tests that use it must be marked
            // #[serial] so no other env-mutating test runs concurrently.
            Some(v) => unsafe { std::env::set_var(key, v) },
            // SAFETY: see the set_var safety note above.
            None => unsafe { std::env::remove_var(key) },
        }
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            // SAFETY: EnvGuard is test-only and callers serialize env mutation.
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            // SAFETY: EnvGuard is test-only and callers serialize env mutation.
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

pub(super) async fn spawn_ask_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let cfg = Arc::new(axon_core::config::Config::default());
    let ctx = Arc::new(ServiceContext::from_runtime(
        Arc::clone(&cfg),
        Arc::new(EmptyRuntime),
    ));
    let app = protect_routes(
        ask_router::<()>(cfg, ctx),
        &auth_policy,
        ScopeRequirement::Write,
    );
    spawn_app(app).await
}

pub(super) async fn spawn_full_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    spawn_full_test_server_with_config(auth_policy, axon_core::config::Config::default()).await
}

pub(super) async fn spawn_full_test_server_with_config(
    auth_policy: AuthPolicy,
    cfg: axon_core::config::Config,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let home = tempfile::tempdir().expect("temp home");
    let home_guard = EnvGuard::set_key("HOME", home.path().to_str());
    let axon_home = home.path().join(".axon");
    std::fs::create_dir_all(&axon_home).expect("create axon home");
    std::fs::write(axon_home.join("panel-password"), b"test-panel-token\n")
        .expect("write panel password");
    let panel = Arc::new(super::PanelRuntimeState::initialize("127.0.0.1", 0).expect("panel"));
    let cfg = Arc::new(cfg);
    let ctx = Arc::new(ServiceContext::from_runtime(
        Arc::clone(&cfg),
        Arc::new(EmptyRuntime),
    ));
    let app = super::router(cfg, panel, ctx, auth_policy);

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
        drop(home_guard);
        drop(home);
    });

    (format!("http://{addr}"), shutdown_tx, handle)
}

pub(super) async fn stop(shutdown: oneshot::Sender<()>, handle: tokio::task::JoinHandle<()>) {
    let _ = shutdown.send(());
    handle.await.expect("server task");
}

#[tokio::test]
#[serial]
async fn panel_artifact_requires_panel_token_and_serves_png() {
    let _guard = EnvGuard::set(Some("api-secret"));
    let temp = tempfile::tempdir().unwrap();
    let artifact = FileArtifactStore::new(temp.path().join("artifacts"))
        .put_bytes(ArtifactBytesWriteRequest {
            kind: ArtifactKind::Screenshot,
            content_type: "image/png".to_string(),
            bytes: b"png-bytes".to_vec(),
            source_id: None,
            job_id: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    let route = format!("/api/panel/artifacts/{}/content", artifact.artifact_id.0);

    let cfg = axon_core::config::Config {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };
    let (base, shutdown, handle) =
        spawn_full_test_server_with_config(AuthPolicy::LoopbackDev, cfg).await;
    let client = reqwest::Client::new();

    let unauthorized = client
        .get(format!("{base}{route}"))
        .send()
        .await
        .expect("unauthorized request");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let bearer_rejected = client
        .get(format!("{base}{route}"))
        .header("authorization", "Bearer api-secret")
        .send()
        .await
        .expect("bearer rejected request");
    assert_eq!(bearer_rejected.status(), StatusCode::UNAUTHORIZED);

    let authorized = client
        .get(format!("{base}{route}"))
        .header("x-axon-panel-token", "test-panel-token")
        .send()
        .await
        .expect("authorized request");
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(
        authorized.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/png"
    );
    assert_eq!(
        authorized
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .unwrap(),
        "nosniff"
    );
    assert_eq!(authorized.bytes().await.unwrap().as_ref(), b"png-bytes");

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn v1_artifacts_use_opaque_ids_and_reject_path_access() {
    let _env = EnvGuard::set(Some("secret"));
    let temp = tempfile::tempdir().unwrap();
    let cfg = axon_core::config::Config {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };
    let (base, shutdown, handle) =
        spawn_full_test_server_with_config(AuthPolicy::Mounted { auth_state: None }, cfg).await;
    let client = reqwest::Client::new();

    let unauthorized = client
        .get(format!("{base}/v1/artifacts"))
        .send()
        .await
        .expect("unauthorized request");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let list = client
        .get(format!("{base}/v1/artifacts"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("artifact list request");
    assert_eq!(list.status(), StatusCode::OK);

    let path_query = client
        .get(format!("{base}/v1/artifacts?path=screenshots/shot.png"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("removed path-query request");
    assert_eq!(path_query.status(), StatusCode::BAD_REQUEST);

    let wildcard = client
        .get(format!("{base}/v1/artifacts/screenshots/shot.png"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("removed wildcard request");
    assert_eq!(wildcard.status(), StatusCode::NOT_FOUND);

    stop(shutdown, handle).await;
}

/// The legacy `/v1/ingest/sessions/prepared` route was removed in #343 (folded
/// into `/v1/sources` + `/v1/uploads`). Before the API-aware 404 fallback
/// landed, an unrouted `/v1/*` path silently returned the SPA `index.html`
/// (200), which masked the removal. It must now return the contract
/// `ErrorEnvelope` 404 (`route.not_found`), not the SPA HTML.
#[tokio::test]
#[serial]
async fn removed_prepared_sessions_route_returns_enveloped_not_found() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "docs": [{
            "url": "file:///tmp/session.jsonl",
            "title": null,
            "text": "x".repeat(140 * 1024),
            "session_platform": "codex",
            "session_project": "axon_rust",
            "session_date": null,
            "session_turn_count": 1,
            "session_file": "/tmp/session.jsonl",
            "extra": {}
        }],
        "project": "axon_rust",
        "collection": "axon_sessions"
    });

    let response = client
        .post(format!("{base}/v1/ingest/sessions/prepared"))
        .bearer_auth("secret")
        .json(&body)
        .send()
        .await
        .expect("prepared sessions request");
    let status = response.status();
    let value: serde_json::Value = response.json().await.expect("enveloped 404 body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(value["ok"], serde_json::json!(false));
    assert_eq!(value["error"]["code"], "route.not_found");
}

async fn spawn_app(
    app: axum::Router,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
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
