#![allow(unsafe_code)]

use super::{ScopeRequirement, ask_router, protect_routes};
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::mcp::auth::AuthPolicy;
use crate::services::context::ServiceContext;
use crate::services::runtime::ServiceJobRuntime;
use crate::services::types::ServiceJob;
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

const ENV_KEY: &str = "AXON_MCP_HTTP_TOKEN";

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

pub(super) async fn spawn_ask_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let app = protect_routes(
        ask_router::<()>(Arc::new(crate::core::config::Config::default())),
        &auth_policy,
        ScopeRequirement::Write,
    );
    spawn_app(app).await
}

pub(super) async fn spawn_full_test_server(
    auth_policy: AuthPolicy,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let home = tempfile::tempdir().expect("temp home");
    let home_guard = EnvGuard::set_key("HOME", home.path().to_str());
    let panel = Arc::new(super::PanelRuntimeState::initialize("127.0.0.1", 0).expect("panel"));
    let cfg = Arc::new(crate::core::config::Config::default());
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
