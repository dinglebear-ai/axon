use crate::crates::core::config::Config;
use std::sync::Arc;

use super::events;

#[derive(Debug, Clone)]
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
