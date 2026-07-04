use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::document::SourceDocument;
use super::enums::*;
use super::ids::*;
use super::lifecycle::JobDescriptor;
use super::listing::{JobSummary, Page, WatchSummary};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactWriteRequest {
    pub kind: ArtifactKind,
    pub content_type: String,
    pub content: ContentRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactHandle {
    pub artifact_id: ArtifactId,
    pub artifact_kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReadResult {
    pub handle: ArtifactHandle,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentRef>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveConfig {
    pub snapshot_id: ConfigSnapshotId,
    pub values: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigValidationReport {
    pub valid: bool,
    pub warnings: Vec<SourceWarning>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct DocumentCacheKey {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CachedDocument {
    pub document: SourceDocument,
    pub cached_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocumentCacheInvalidation {
    Source { source_id: SourceId },
    Generation { generation: SourceGenerationId },
    Key { key: DocumentCacheKey },
    All,
}

impl<'de> Deserialize<'de> for DocumentCacheInvalidation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("expected document cache invalidation object"))?;
        let kind = object
            .get("kind")
            .and_then(|value| value.as_str())
            .ok_or_else(|| D::Error::custom("missing document cache invalidation kind"))?;
        match kind {
            "source" => {
                reject_unknown_keys::<D::Error>(object, &["kind", "source_id"])?;
                Ok(Self::Source {
                    source_id: deserialize_field::<D::Error, SourceId>(object, "source_id")?,
                })
            }
            "generation" => {
                reject_unknown_keys::<D::Error>(object, &["kind", "generation"])?;
                Ok(Self::Generation {
                    generation: deserialize_field::<D::Error, SourceGenerationId>(
                        object,
                        "generation",
                    )?,
                })
            }
            "key" => {
                reject_unknown_keys::<D::Error>(object, &["kind", "key"])?;
                Ok(Self::Key {
                    key: deserialize_field::<D::Error, DocumentCacheKey>(object, "key")?,
                })
            }
            "all" => {
                reject_unknown_keys::<D::Error>(object, &["kind"])?;
                Ok(Self::All)
            }
            other => Err(D::Error::unknown_variant(
                other,
                &["source", "generation", "key", "all"],
            )),
        }
    }
}

fn deserialize_field<E, T>(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &'static str,
) -> Result<T, E>
where
    E: serde::de::Error,
    T: DeserializeOwned,
{
    object
        .get(field)
        .cloned()
        .ok_or_else(|| E::missing_field(field))
        .and_then(|value| serde_json::from_value(value).map_err(E::custom))
}

fn reject_unknown_keys<E>(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &'static [&'static str],
) -> Result<(), E>
where
    E: serde::de::Error,
{
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(E::unknown_field(key, allowed));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchHistoryRequest {
    pub watch_id: WatchId,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchHistoryResult {
    pub runs: Vec<JobDescriptor>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RateLimitRequest {
    pub provider_id: ProviderId,
    pub units: u32,
    pub priority: JobPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RateLimitPermit {
    pub provider_id: ProviderId,
    pub units: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthProbeRequest {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityPolicyRequest {
    pub caller: CallerContext,
    pub safety_class: SafetyClass,
    pub target: String,
}

pub type JobSummaryPage = Page<JobSummary>;
pub type WatchSummaryPage = Page<WatchSummary>;
