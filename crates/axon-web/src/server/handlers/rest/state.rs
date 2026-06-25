//! Shared state for REST handlers.
//!
//! Mirrors `crate::actions::ActionState`: a cloneable handle that lazily
//! initializes the `ServiceContext` (with workers) on first use. Used by every
//! family of REST routes.

use axon_core::config::Config;
use axon_jobs::store::open_config_pool;
use axon_mcp::auth::AuthPolicy;
use axon_services::context::ServiceContext;
use sqlx::SqlitePool;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone)]
pub(crate) struct RestState {
    pub(crate) cfg: Arc<Config>,
    pub(crate) service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    fallback_watch_pool: Arc<OnceCell<Arc<SqlitePool>>>,
    pub(crate) auth_required: bool,
}

impl RestState {
    pub(crate) fn new(
        cfg: Arc<Config>,
        service_context: Arc<OnceCell<Arc<ServiceContext>>>,
        auth_policy: &AuthPolicy,
    ) -> Self {
        Self {
            cfg,
            service_context,
            fallback_watch_pool: Arc::new(OnceCell::new()),
            auth_required: !matches!(auth_policy, AuthPolicy::LoopbackDev),
        }
    }

    /// Lazily initialize and return the per-server [`ServiceContext`].
    pub(crate) async fn service_context(
        &self,
    ) -> Result<Arc<ServiceContext>, Box<dyn Error + Send + Sync>> {
        self.service_context
            .get_or_try_init(|| async {
                ServiceContext::new_with_workers(Arc::clone(&self.cfg))
                    .await
                    .map(Arc::new)
            })
            .await
            .map(Arc::clone)
    }

    /// Return the SQLite pool used by watch endpoints.
    ///
    /// Prefer the ServiceContext job-runtime pool so long-lived REST surfaces
    /// use one SQLite pool for jobs and watches. The fallback is retained for
    /// tests or future non-SQLite runtimes.
    pub(crate) async fn watch_pool(&self) -> Result<Arc<SqlitePool>, Box<dyn Error + Send + Sync>> {
        let context = self.service_context().await?;
        if let Some(pool) = context.jobs.sqlite_pool() {
            return Ok(pool);
        }

        self.fallback_watch_pool
            .get_or_try_init(|| async { open_config_pool(&self.cfg).await.map(Arc::new) })
            .await
            .map(Arc::clone)
            .map_err(|err| -> Box<dyn Error + Send + Sync> { err.into() })
    }
}
