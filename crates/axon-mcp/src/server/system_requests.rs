use axon_api::mcp_schema::ResponseMode;
use axon_api::mcp_schema::WatchSubaction;
use axon_api::source::{ContentRef, LifecycleStatus, MetadataMap, UploadPurpose, UploadStatusKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum McpSystemRequest {
    Reset(ResetMcpRequest),
    Collections(CollectionsMcpRequest),
    Uploads(UploadsMcpRequest),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ResetMcpRequest {
    pub subaction: Option<ResetSubaction>,
    pub stores: Option<Vec<String>>,
    pub collection: Option<String>,
    pub include_artifacts: Option<bool>,
    pub include_config: Option<bool>,
    pub reason: Option<String>,
    pub plan_id: Option<String>,
    pub confirm: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResetSubaction {
    #[default]
    Plan,
    Exec,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CollectionsMcpRequest {
    pub subaction: Option<CollectionsSubaction>,
    pub collection: Option<String>,
    pub prefix: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum CollectionsSubaction {
    #[default]
    List,
    Get,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct UploadsMcpRequest {
    pub subaction: Option<UploadsSubaction>,
    pub upload_id: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub purpose: Option<UploadPurpose>,
    pub sha256: Option<String>,
    pub source_hint: Option<String>,
    pub content: Option<String>,
    pub content_ref: Option<ContentRef>,
    pub source_options: Option<MetadataMap>,
    pub reason: Option<String>,
    pub status: Option<UploadStatusKind>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum UploadsSubaction {
    #[default]
    List,
    Create,
    Get,
    PutContent,
    Complete,
    Abort,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum McpWatchRequest {
    Watch(WatchMcpRequest),
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WatchMcpRequest {
    pub subaction: Option<WatchSubaction>,
    pub id: Option<String>,
    pub every_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub status: Option<LifecycleStatus>,
    pub collection: Option<String>,
    pub source: Option<String>,
    pub embed: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

#[cfg(test)]
#[path = "system_requests_tests.rs"]
mod tests;
