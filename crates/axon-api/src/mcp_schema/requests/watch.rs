//! Watch action request types (issue #298 WS-B). Extracted from
//! `requests.rs` to keep it under the monolith line cap.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ResponseMode;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub subaction: Option<WatchSubaction>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub task_type: Option<String>,
    pub task_payload: Option<Value>,
    pub every_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    /// Target collection for the source-request-backed watch store
    /// (`subaction=update`). Unused by the legacy create/list/get/exec/history
    /// subactions above.
    pub collection: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WatchSubaction {
    Create,
    List,
    Get,
    Exec,
    History,
    /// Update a source-request-backed watch (issue #298 WS-B). Distinct
    /// storage model from `create`/`list`/`get`/`exec`/`history` above — see
    /// `crates/axon-jobs/src/watch_store.rs` module docs.
    Update,
    /// Disable a source-request-backed watch's scheduler execution.
    Pause,
    /// Re-enable a source-request-backed watch's scheduler execution.
    Resume,
    /// Hard-delete a source-request-backed watch and its run history.
    Delete,
}
