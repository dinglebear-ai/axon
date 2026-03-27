use crate::crates::core::config::Config;
use crate::crates::services::context::ServiceContext;
use std::sync::Arc;

use super::events;

#[derive(Clone)]
pub(crate) struct ExecCommandContext {
    pub(crate) exec_id: String,
    pub(crate) mode: String,
    pub(crate) input: String,
    /// Raw flags from the WS request.  Both `sync_mode` and `async_mode` call
    /// `cfg.apply_overrides(...)` with values derived from these flags to
    /// produce a per-request `Config` for direct service dispatch.
    pub(crate) flags: serde_json::Value,
    /// Base server config from `AppState`.
    pub(crate) cfg: Arc<Config>,
    /// Shared service runtime from `AppState`.
    pub(crate) service_context: Arc<ServiceContext>,
}

impl ExecCommandContext {
    pub(super) fn to_ws_ctx(&self) -> events::CommandContext {
        events::CommandContext {
            exec_id: self.exec_id.clone(),
            mode: self.mode.clone(),
            input: self.input.clone(),
        }
    }
}
