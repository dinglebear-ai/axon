use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::capability::RenderMode;
use super::common::*;
use super::enums::CredentialKind;
use super::ids::*;

/// Result recency filter for [`SearchRequest`]. Mirrors `spider_agent::TimeRange`'s
/// four named variants (its `Custom { start, end }` range is not exposed here —
/// no axon caller has ever constructed one; see `ServiceTimeRange` in
/// `axon-services`, which has the same four-variant shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    pub limit: u32,
    /// Number of leading results to skip before `limit` is applied.
    /// `0` (the default) returns the first page.
    #[serde(default)]
    pub offset: u32,
    /// Restrict results to a recency window (provider-dependent support).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<SearchTimeRange>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub query: String,
    pub results: Vec<SearchResultItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResultItem {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchRequest {
    pub uri: String,
    pub method: String,
    pub headers: RedactedHeaders,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_refs: Vec<SecretRef>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchedResource {
    pub uri: String,
    pub final_uri: String,
    pub status: u16,
    pub content: ContentRef,
    pub headers: RedactedHeaders,
    pub fetched_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirect_chain: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub uri: String,
    pub mode: RenderMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation_script: Option<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_refs: Vec<SecretRef>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderedResource {
    pub uri: String,
    pub final_uri: String,
    pub markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub render_mode: RenderMode,
    pub captured_at: Timestamp,
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub console: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network: Vec<NetworkCaptureEntry>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkCaptureRequest {
    pub uri: String,
    #[serde(default)]
    pub include_request_headers: bool,
    #[serde(default)]
    pub include_response_headers: bool,
    #[serde(default)]
    pub include_bodies: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkCaptureResult {
    pub uri: String,
    pub captured_at: Timestamp,
    pub entries: Vec<NetworkCaptureEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkCaptureEntry {
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub request_headers: RedactedHeaders,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<RedactedHeaders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<ContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<ContentRef>,
    pub started_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialRequest {
    pub credential_kind: CredentialKind,
    pub secret_ref: SecretRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialMaterial {
    pub secret_ref: SecretRef,
    pub credential_kind: CredentialKind,
    pub redacted_value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}
