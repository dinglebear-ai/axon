//! VerticalContext — narrowed service context passed to every extractor.
//!
//! Verticals receive this instead of a full `&ServiceContext` so they can't
//! accidentally call unrelated services. The HTTP client is the shared
//! static singleton from `core::http` — SSRF-guarded, pooled, never
//! re-created per call.

use crate::core::config::Config;
use std::sync::Arc;

/// Narrowed view over `ServiceContext` for vertical extractors.
///
/// Contains exactly what an extractor needs: config (for credentials,
/// timeouts, collection names) and nothing else. Extractors MUST NOT
/// perform raw HTTP fetches — use `http_client()` from `crate::core::http`
/// inside the extractor, which goes through the SSRF guard.
#[derive(Clone)]
pub struct VerticalContext {
    pub cfg: Arc<Config>,
}

impl VerticalContext {
    pub fn new(cfg: Arc<Config>) -> Self {
        Self { cfg }
    }
}

impl From<&crate::services::context::ServiceContext> for VerticalContext {
    fn from(ctx: &crate::services::context::ServiceContext) -> Self {
        Self {
            cfg: Arc::clone(&ctx.cfg),
        }
    }
}
