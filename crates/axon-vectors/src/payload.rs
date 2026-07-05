//! Validated vector payload metadata.

use std::fmt;

use axon_api::source::{ChunkLocator, MetadataMap, SourceRange};
use serde_json::Value;

use crate::payload_redaction::{forbidden_field_name, validate_forbidden_value};

pub use crate::payload_generation::generation_payload_i64;
pub use crate::payload_redaction::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
};

pub const MODULE_NAME: &str = "payload";
pub const VECTOR_PAYLOAD_CONTRACT_VERSION: &str = "2026-07-01";
pub const SOURCE_RANGE_ANCHOR_FIELDS: &[&str] = &[
    "line_start",
    "line_end",
    "byte_start",
    "byte_end",
    "char_start",
    "char_end",
    "time_start_ms",
    "time_end_ms",
    "csv_row",
    "dom_selector",
    "json_pointer",
    "yaml_path",
    "xml_xpath",
    "session_turn_id",
    "turn_start",
    "turn_end",
];

#[derive(Debug, Clone, PartialEq)]
pub struct VectorPayload {
    metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorPayloadValidationError {
    MissingRequiredField { field: String },
    ForbiddenField { field: String },
    ForbiddenValue { field: String },
    UnknownSourceSpecificField { field: String },
    InvalidGeneration { field: String },
    InvalidContractVersion,
    InvalidSourceFamily,
    InvalidVisibility,
    InvalidRedactionStatus,
    InvalidFieldShape { field: String },
}

impl fmt::Display for VectorPayloadValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequiredField { field } => {
                write!(f, "missing required vector payload field `{field}`")
            }
            Self::ForbiddenField { field } => {
                write!(f, "forbidden vector payload field `{field}`")
            }
            Self::ForbiddenValue { field } => {
                write!(f, "forbidden vector payload value under `{field}`")
            }
            Self::UnknownSourceSpecificField { field } => {
                write!(
                    f,
                    "unknown vector payload field `{field}` for source family"
                )
            }
            Self::InvalidGeneration { field } => {
                write!(
                    f,
                    "vector payload field `{field}` must be a non-negative integer or null when allowed"
                )
            }
            Self::InvalidContractVersion => {
                write!(f, "invalid vector payload contract version")
            }
            Self::InvalidSourceFamily => write!(f, "invalid vector payload source_family"),
            Self::InvalidVisibility => write!(f, "invalid vector payload visibility"),
            Self::InvalidRedactionStatus => write!(f, "invalid vector payload redaction_status"),
            Self::InvalidFieldShape { field } => {
                write!(f, "invalid vector payload field shape `{field}`")
            }
        }
    }
}

impl std::error::Error for VectorPayloadValidationError {}

impl VectorPayload {
    pub fn try_from_metadata(metadata: MetadataMap) -> Result<Self, VectorPayloadValidationError> {
        validate_required_fields(&metadata)?;
        validate_forbidden_fields(&metadata)?;
        validate_generations(&metadata)?;
        validate_contract_version(&metadata)?;
        validate_source_family(&metadata)?;
        validate_visibility(&metadata)?;
        validate_redaction_status(&metadata)?;
        validate_shapes(&metadata)?;
        validate_known_fields(&metadata)?;
        Ok(Self { metadata })
    }

    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }

    pub fn into_metadata(self) -> MetadataMap {
        self.metadata
    }
}

fn validate_required_fields(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    for field in REQUIRED_FIELDS {
        if !metadata.contains_key(*field) {
            return Err(VectorPayloadValidationError::MissingRequiredField {
                field: (*field).to_string(),
            });
        }
    }
    Ok(())
}

fn validate_forbidden_fields(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    for (field, value) in metadata.iter() {
        if forbidden_field_name(field) {
            return Err(VectorPayloadValidationError::ForbiddenField {
                field: field.clone(),
            });
        }
        validate_forbidden_value(field, value)?;
    }
    Ok(())
}

fn validate_generations(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    if !metadata
        .get("source_generation")
        .and_then(Value::as_i64)
        .is_some_and(|value| value >= 0)
    {
        return Err(VectorPayloadValidationError::InvalidGeneration {
            field: "source_generation".to_string(),
        });
    }
    let Some(committed_generation) = metadata.get("committed_generation") else {
        return Err(VectorPayloadValidationError::InvalidGeneration {
            field: "committed_generation".to_string(),
        });
    };
    if !committed_generation.is_null()
        && !committed_generation
            .as_i64()
            .is_some_and(|value| value >= 0)
    {
        return Err(VectorPayloadValidationError::InvalidGeneration {
            field: "committed_generation".to_string(),
        });
    }
    Ok(())
}

fn validate_contract_version(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    if metadata
        .get("payload_contract_version")
        .and_then(|value| value.as_str())
        == Some(VECTOR_PAYLOAD_CONTRACT_VERSION)
    {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidContractVersion)
    }
}

fn validate_source_family(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    let value = metadata
        .get("source_family")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if VECTOR_SOURCE_FAMILIES.contains(&value) {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidSourceFamily)
    }
}

fn validate_visibility(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    let value = metadata
        .get("visibility")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if VECTOR_VISIBILITY_VALUES.contains(&value) {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidVisibility)
    }
}

fn validate_redaction_status(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    let value = metadata
        .get("redaction_status")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if VECTOR_REDACTION_STATUS_VALUES.contains(&value) {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidRedactionStatus)
    }
}

fn validate_shapes(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    for field in [
        "payload_contract_version",
        "collection",
        "vector_point_id",
        "vector_namespace",
        "source_family",
        "source_id",
        "source_canonical_uri",
        "document_id",
        "chunk_id",
        "content_kind",
        "content_hash",
        "chunk_hash",
        "chunk_text",
        "job_id",
        "document_status",
        "embedding_model",
        "embedding_provider",
        "embedding_profile",
        "embedded_at",
        "redaction_status",
    ] {
        require_non_empty_string(metadata, field)?;
    }
    require_positive_integer(metadata, "embedding_dimensions")?;

    let locator: ChunkLocator =
        serde_json::from_value(metadata.get("chunk_locator").cloned().ok_or_else(|| {
            VectorPayloadValidationError::InvalidFieldShape {
                field: "chunk_locator".to_string(),
            }
        })?)
        .map_err(|_| VectorPayloadValidationError::InvalidFieldShape {
            field: "chunk_locator".to_string(),
        })?;
    if locator.canonical_uri.trim().is_empty() {
        return Err(VectorPayloadValidationError::InvalidFieldShape {
            field: "chunk_locator.canonical_uri".to_string(),
        });
    }
    validate_source_range_shape(&locator.range, "chunk_locator.range")?;

    let range: SourceRange =
        serde_json::from_value(metadata.get("source_range").cloned().ok_or_else(|| {
            VectorPayloadValidationError::InvalidFieldShape {
                field: "source_range".to_string(),
            }
        })?)
        .map_err(|_| VectorPayloadValidationError::InvalidFieldShape {
            field: "source_range".to_string(),
        })?;
    validate_source_range_shape(&range, "source_range")?;
    Ok(())
}

fn validate_source_range_shape(
    range: &SourceRange,
    field: &str,
) -> Result<(), VectorPayloadValidationError> {
    if source_range_has_anchor(range) {
        validate_source_range_order(range, field)?;
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidFieldShape {
            field: field.to_string(),
        })
    }
}

fn source_range_has_anchor(range: &SourceRange) -> bool {
    range.line_start.is_some()
        || range.line_end.is_some()
        || range.byte_start.is_some()
        || range.byte_end.is_some()
        || range.char_start.is_some()
        || range.char_end.is_some()
        || range.time_start_ms.is_some()
        || range.time_end_ms.is_some()
        || range.csv_row.is_some()
        || non_empty(range.dom_selector.as_deref())
        || non_empty(range.json_pointer.as_deref())
        || non_empty(range.yaml_path.as_deref())
        || non_empty(range.xml_xpath.as_deref())
        || non_empty(range.session_turn_id.as_deref())
        || non_empty(range.turn_start.as_deref())
        || non_empty(range.turn_end.as_deref())
}

fn validate_source_range_order(
    range: &SourceRange,
    field: &str,
) -> Result<(), VectorPayloadValidationError> {
    if let Some(suffix) = [
        range_starts_after(range.line_start, range.line_end, "line"),
        range_starts_after(range.byte_start, range.byte_end, "byte"),
        range_starts_after(range.char_start, range.char_end, "char"),
        range_starts_after(range.time_start_ms, range.time_end_ms, "time_ms"),
    ]
    .into_iter()
    .flatten()
    .next()
    {
        return Err(VectorPayloadValidationError::InvalidFieldShape {
            field: format!("{field}.{suffix}"),
        });
    }
    Ok(())
}

fn range_starts_after<T: Ord>(start: Option<T>, end: Option<T>, prefix: &str) -> Option<String> {
    start
        .zip(end)
        .is_some_and(|(start, end)| start > end)
        .then(|| format!("{prefix}_start_gt_end"))
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn require_non_empty_string(
    metadata: &MetadataMap,
    field: &str,
) -> Result<(), VectorPayloadValidationError> {
    if metadata
        .get(field)
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty())
    {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidFieldShape {
            field: field.to_string(),
        })
    }
}

fn require_positive_integer(
    metadata: &MetadataMap,
    field: &str,
) -> Result<(), VectorPayloadValidationError> {
    if metadata
        .get(field)
        .and_then(|value| value.as_i64())
        .is_some_and(|value| value > 0)
    {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidFieldShape {
            field: field.to_string(),
        })
    }
}

fn validate_known_fields(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    let source_family = metadata
        .get("source_family")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    for field in metadata.keys() {
        if SHARED_FIELDS.contains(&field.as_str()) {
            continue;
        }
        if source_family_allows_field(source_family, field) {
            continue;
        }
        return Err(VectorPayloadValidationError::UnknownSourceSpecificField {
            field: field.clone(),
        });
    }
    Ok(())
}

pub fn source_family_allows_field(source_family: &str, field: &str) -> bool {
    VECTOR_SOURCE_FAMILY_FIELDS
        .iter()
        .find(|(family, _)| *family == source_family)
        .is_some_and(|(_, fields)| fields.contains(&field))
}

pub const VECTOR_REQUIRED_FIELDS: &[&str] = &[
    "payload_contract_version",
    "collection",
    "vector_point_id",
    "vector_namespace",
    "source_family",
    "source_kind",
    "source_adapter",
    "source_scope",
    "source_id",
    "source_canonical_uri",
    "source_item_key",
    "item_canonical_uri",
    "source_generation",
    "document_id",
    "chunk_id",
    "content_kind",
    "content_hash",
    "chunk_hash",
    "chunk_text",
    "chunk_locator",
    "source_range",
    "visibility",
    "redaction_status",
    "job_id",
    "document_status",
    "embedding_model",
    "embedding_dimensions",
    "embedding_provider",
    "embedding_profile",
    "embedded_at",
    "committed_generation",
];

pub const VECTOR_VISIBILITY_VALUES: &[&str] =
    &["public", "internal", "sensitive", "redacted", "derived"];

pub const VECTOR_REDACTION_STATUS_VALUES: &[&str] = &["clean", "redacted", "failed"];

pub use crate::payload_families::{VECTOR_SOURCE_FAMILIES, VECTOR_SOURCE_FAMILY_FIELDS};

pub const VECTOR_SHARED_FIELDS: &[&str] = &[
    "payload_contract_version",
    "collection",
    "vector_point_id",
    "source_family",
    "source_type",
    "source_kind",
    "source_adapter",
    "source_scope",
    "source_id",
    "source_canonical_uri",
    "source_item_key",
    "item_canonical_uri",
    "source_generation",
    "committed_generation",
    "document_id",
    "chunk_id",
    "chunk_key",
    "content_hash",
    "chunk_hash",
    "chunk_text",
    "content_kind",
    "chunk_content_kind",
    "chunking_fallback",
    "chunking_fallback_from",
    "code_chunk_source",
    "chunk_locator",
    "source_range",
    "vector_namespace",
    "visibility",
    "redaction_status",
    "job_id",
    "document_status",
    "embedding_batch_id",
    "embedding_model",
    "embedding_dimensions",
    "embedding_provider",
    "embedding_profile",
    "embedded_at",
];

const REQUIRED_FIELDS: &[&str] = VECTOR_REQUIRED_FIELDS;
const SHARED_FIELDS: &[&str] = VECTOR_SHARED_FIELDS;
