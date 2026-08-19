use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::SourceWarning;
use super::ids::{
    ArtifactCandidateId, JobId, MetadataMap, SourceGenerationId, SourceId, Timestamp,
};

/// Shared neutral ArtifactCandidate payload schema owned by the cross-repo G0 contract.
/// Axon is a producer of this payload, not its publication/domain authority.
pub const ARTIFACT_CANDIDATE_SCHEMA_VERSION: &str = "dinglebear.artifact-candidate/v1";

/// Axon-specific transport-batch version. This is deliberately separate from
/// ARTIFACT_CANDIDATE_SCHEMA_VERSION, which versions each neutral candidate.
pub const ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION: &str = "1";

/// Media type for the Axon transport wrapper around neutral ArtifactCandidate payloads.
pub const ARTIFACT_CANDIDATE_MEDIA_TYPE: &str =
    "application/vnd.dinglebear.axon.artifact-candidates+json;version=1";

pub const ARTIFACT_CANDIDATE_MAX_BYTES: usize = 262_144;
pub const ARTIFACT_CANDIDATE_MAX_KIND_HINTS: usize = 32;
pub const ARTIFACT_CANDIDATE_MAX_OBSERVED_FILES_BYTES: usize = 65_536;
pub const ARTIFACT_CANDIDATE_MAX_MANIFEST_METADATA_BYTES: usize = 65_536;
pub const ARTIFACT_CANDIDATE_MAX_DISCOVERY_EVIDENCE_BYTES: usize = 65_536;
pub const ARTIFACT_CANDIDATE_MAX_POPULARITY_SIGNALS_BYTES: usize = 32_768;
pub const ARTIFACT_CANDIDATE_MAX_LICENSE_EVIDENCE_BYTES: usize = 65_536;
pub const ARTIFACT_CANDIDATE_MAX_CONTENT_DIGESTS: usize = 2_000;
pub const ARTIFACT_CANDIDATE_MAX_WARNINGS: usize = 128;

const MAX_JSON_DEPTH: usize = 8;
const MAX_MAP_ENTRIES: usize = 128;
const MAX_LIST_ENTRIES: usize = 256;
const MAX_JSON_STRING_BYTES: usize = 16_384;
const SECRET_KEYS: &[&str] = &[
    "authorization",
    "password",
    "passwd",
    "token",
    "secret",
    "api_key",
    "apikey",
    "credential",
    "cookie",
];

/// Stable and content-aware keys used by Axon while producing neutral candidates.
/// This helper is not a field in the shared ArtifactCandidate wire schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCandidateDedupe {
    pub identity_key: String,
    pub content_key: Option<String>,
    pub content_hash: Option<String>,
}

/// Shared neutral ArtifactCandidate payload.
/// Field names intentionally mirror the Depot-owned G0 fixture exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactCandidate {
    pub schema_version: String,
    pub id: ArtifactCandidateId,
    pub canonical_source_uri: String,
    pub source_provider: String,
    pub observed_at: Timestamp,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default, rename = "ref")]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub kind_hints: Vec<String>,
    #[serde(default)]
    pub observed_files: Vec<serde_json::Value>,
    #[serde(default)]
    pub manifest_metadata: MetadataMap,
    #[serde(default)]
    pub content_digests: Vec<String>,
    #[serde(default)]
    pub discovery_evidence: MetadataMap,
    #[serde(default)]
    pub popularity_signals: MetadataMap,
    #[serde(default)]
    pub license_evidence: MetadataMap,
    #[serde(default)]
    pub crawl_generation_id: Option<String>,
    #[serde(default)]
    pub crawl_job_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl ArtifactCandidate {
    /// Validate the effective bounds and secret-safe JSON rules of the neutral
    /// G0 candidate contract before an Axon sink can deliver this payload.
    pub fn validate_shared_contract(&self) -> Result<(), String> {
        if self.schema_version != ARTIFACT_CANDIDATE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported ArtifactCandidate schemaVersion {}",
                self.schema_version
            ));
        }
        validate_id(&self.id.0)?;
        validate_source_uri(&self.canonical_source_uri)?;
        validate_kind(&self.source_provider, "sourceProvider")?;
        validate_timestamp(&self.observed_at.0)?;
        validate_optional_string(self.repository.as_deref(), 512, "repository")?;
        validate_optional_string(self.source_ref.as_deref(), 512, "ref")?;
        validate_source_path(self.source_path.as_deref())?;

        if self.kind_hints.len() > ARTIFACT_CANDIDATE_MAX_KIND_HINTS {
            return Err("kindHints exceeds 32 entries".to_string());
        }
        for hint in &self.kind_hints {
            validate_kind(hint, "kindHints")?;
        }

        validate_json_value(
            &serde_json::Value::Array(self.observed_files.clone()),
            "observedFiles",
            ARTIFACT_CANDIDATE_MAX_OBSERVED_FILES_BYTES,
        )?;
        validate_json_value(
            &serde_json::to_value(&self.manifest_metadata).map_err(|error| error.to_string())?,
            "manifestMetadata",
            ARTIFACT_CANDIDATE_MAX_MANIFEST_METADATA_BYTES,
        )?;
        validate_json_value(
            &serde_json::to_value(&self.discovery_evidence).map_err(|error| error.to_string())?,
            "discoveryEvidence",
            ARTIFACT_CANDIDATE_MAX_DISCOVERY_EVIDENCE_BYTES,
        )?;
        validate_json_value(
            &serde_json::to_value(&self.popularity_signals).map_err(|error| error.to_string())?,
            "popularitySignals",
            ARTIFACT_CANDIDATE_MAX_POPULARITY_SIGNALS_BYTES,
        )?;
        validate_json_value(
            &serde_json::to_value(&self.license_evidence).map_err(|error| error.to_string())?,
            "licenseEvidence",
            ARTIFACT_CANDIDATE_MAX_LICENSE_EVIDENCE_BYTES,
        )?;

        if self.content_digests.len() > ARTIFACT_CANDIDATE_MAX_CONTENT_DIGESTS {
            return Err("contentDigests exceeds 2000 entries".to_string());
        }
        for digest in &self.content_digests {
            validate_digest(digest)?;
        }
        validate_optional_string(
            self.crawl_generation_id.as_deref(),
            256,
            "crawlGenerationId",
        )?;
        validate_optional_string(self.crawl_job_id.as_deref(), 256, "crawlJobId")?;
        if self.warnings.len() > ARTIFACT_CANDIDATE_MAX_WARNINGS {
            return Err("warnings exceeds 128 entries".to_string());
        }
        for warning in &self.warnings {
            validate_string(warning, 1_024, "warning", true)?;
        }

        let value = serde_json::to_value(self).map_err(|error| error.to_string())?;
        validate_json_shape(&value, 0)?;
        let encoded = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        if encoded.len() > ARTIFACT_CANDIDATE_MAX_BYTES {
            return Err("ArtifactCandidate exceeds 262144 bytes".to_string());
        }
        Ok(())
    }

    /// Conservative Axon-side mirroring guard over neutral license evidence.
    pub fn permits_public_byte_mirroring(&self) -> bool {
        self.license_evidence
            .get("redistribution")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| matches!(value, "redistributable" | "forkable"))
    }
}

/// Versioned Axon delivery envelope sent to a candidate sink such as Depot.
/// This is a transport wrapper only; each candidate has its own neutral schemaVersion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateBatch {
    pub contract_version: String,
    pub delivery_id: String,
    pub idempotency_key: String,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub produced_at: Timestamp,
    pub candidates: Vec<ArtifactCandidate>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCandidateSinkStatus {
    Disabled,
    Accepted,
    Partial,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateSinkResult {
    pub status: ArtifactCandidateSinkStatus,
    pub attempted: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub warnings: Vec<SourceWarning>,
}

/// Capability for the Axon batch wrapper, not the neutral candidate schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateSinkCapability {
    pub name: String,
    pub version: String,
    pub contract_versions: Vec<String>,
    pub max_batch_size: u32,
    pub supports_idempotency: bool,
}

fn validate_id(value: &str) -> Result<(), String> {
    validate_string(value, 160, "id", false)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("id is empty".to_string());
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("id has invalid first character".to_string());
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        return Err("id contains invalid characters".to_string());
    }
    Ok(())
}

fn validate_kind(value: &str, field: &str) -> Result<(), String> {
    validate_string(value, 64, field, false)?;
    if value.split('-').any(|segment| {
        segment.is_empty()
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    }) {
        return Err(format!("{field} is not a lowercase kebab-case kind"));
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<(), String> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err("content digest must use sha256:".to_string());
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err("content digest must contain 64 lowercase hex characters".to_string());
    }
    Ok(())
}

fn validate_source_uri(value: &str) -> Result<(), String> {
    validate_string(value, 4_096, "canonicalSourceUri", false)?;
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err("canonicalSourceUri must have a URI scheme".to_string());
    };
    if scheme.is_empty()
        || matches!(
            scheme.to_ascii_lowercase().as_str(),
            "file" | "data" | "javascript"
        )
    {
        return Err("canonicalSourceUri uses an invalid or blocked scheme".to_string());
    }
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return Err("canonicalSourceUri must not contain credentials".to_string());
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<(), String> {
    validate_string(value, 64, "observedAt", false)?;
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| "observedAt must be an ISO-8601 timestamp".to_string())
}

fn validate_source_path(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    validate_string(value, 4_096, "sourcePath", false)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value.get(1..2) == Some(":")
        || value
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
    {
        return Err("sourcePath must be a safe relative slash path".to_string());
    }
    Ok(())
}

fn validate_optional_string(value: Option<&str>, max: usize, field: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    validate_string(value, max, field, true)
}

fn validate_string(value: &str, max: usize, field: &str, allow_empty: bool) -> Result<(), String> {
    if !allow_empty && value.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > max {
        return Err(format!("{field} exceeds {max} bytes"));
    }
    if value.as_bytes().contains(&0) {
        return Err(format!("{field} contains a NUL byte"));
    }
    Ok(())
}

fn validate_json_value(
    value: &serde_json::Value,
    field: &str,
    max_bytes: usize,
) -> Result<(), String> {
    validate_json_shape(value, 0)?;
    let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if encoded.len() > max_bytes {
        return Err(format!("{field} exceeds {max_bytes} JSON bytes"));
    }
    Ok(())
}

fn validate_json_shape(value: &serde_json::Value, depth: usize) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err("candidate JSON exceeds max depth 8".to_string());
    }
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            Ok(())
        }
        serde_json::Value::String(value) => {
            if value.len() > MAX_JSON_STRING_BYTES {
                return Err("candidate JSON string exceeds 16384 bytes".to_string());
            }
            if value.as_bytes().contains(&0) {
                return Err("candidate JSON string contains a NUL byte".to_string());
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            if values.len() > MAX_LIST_ENTRIES {
                return Err("candidate JSON list exceeds 256 entries".to_string());
            }
            for value in values {
                validate_json_shape(value, depth + 1)?;
            }
            Ok(())
        }
        serde_json::Value::Object(map) => {
            if map.len() > MAX_MAP_ENTRIES {
                return Err("candidate JSON map exceeds 128 entries".to_string());
            }
            for (key, value) in map {
                if key.len() > 128 {
                    return Err("candidate metadata key exceeds 128 bytes".to_string());
                }
                if is_secret_key(key) {
                    return Err(format!("candidate metadata contains secret-like key {key}"));
                }
                validate_json_shape(value, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn is_secret_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let compact = normalized.replace('_', "");
    SECRET_KEYS.iter().any(|secret| {
        let compact_secret = secret.replace('_', "");
        normalized == *secret
            || compact == compact_secret
            || normalized.ends_with(&format!("_{secret}"))
            || compact.ends_with(&compact_secret)
    })
}
