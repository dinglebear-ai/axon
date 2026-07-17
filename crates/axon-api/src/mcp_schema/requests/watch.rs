//! Watch action request types (issue #298 WS-B). Extracted from
//! `requests.rs` to keep it under the monolith line cap.

use serde::{Deserialize, Serialize};

use super::ResponseMode;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub subaction: Option<WatchSubaction>,
    /// Watch id for get/status/update/pause/resume/delete/exec/history.
    /// `status`, `exec`, and `history` also accept `source` when the caller does
    /// not know the id.
    pub id: Option<String>,
    pub every_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    /// Target collection for the source-request-backed watch store
    /// (`subaction=update`/`subaction=create`).
    pub collection: Option<String>,
    /// Source URI to watch (issue #298 WS-B `subaction=create`), e.g.
    /// `https://example.com/docs` or `file:///path/to/repo`. Required for
    /// `subaction=create`; unused otherwise.
    pub source: Option<String>,
    /// Whether the source-request-backed watch (`subaction=create`) should
    /// embed content on each run. Defaults to `true` when omitted.
    pub embed: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WatchSubaction {
    Create,
    List,
    Get,
    Status,
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
