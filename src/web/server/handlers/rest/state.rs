//! Shared state for REST handlers.
//!
//! Mirrors `crate::web::actions::ActionState`: a cloneable handle that lazily
//! initializes the `ServiceContext` (with workers) on first use. Used by every
//! family of REST routes.

use crate::core::config::Config;
use crate::mcp::auth::AuthPolicy;
use crate::services::context::ServiceContext;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone)]
pub(crate) struct RestState {
    pub(crate) cfg: Arc<Config>,
    pub(crate) service_context: Arc<OnceCell<Arc<ServiceContext>>>,
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
}
