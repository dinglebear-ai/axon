//! Validated vector payload metadata.

use std::fmt;

use axon_api::reset::TARGET_PAYLOAD_CONTRACT_VERSION;
use axon_api::source::MetadataMap;
use serde_json::Value;

use crate::payload_redaction::{forbidden_field_name, validate_forbidden_value};

pub use crate::payload_generation::generation_payload_i64;
pub use crate::payload_redaction::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
};

pub const MODULE_NAME: &str = "payload";
pub const VECTOR_PAYLOAD_CONTRACT_VERSION: &str = TARGET_PAYLOAD_CONTRACT_VERSION;
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
    if metadata
        .get("source_generation")
        .and_then(Value::as_i64)
        .is_none_or(|value| value < 0)
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
        && committed_generation.as_i64().is_none_or(|value| value < 0)
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

use crate::payload_shape::validate_shapes;

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
    "chunk_index",
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
    "chunking_profile",
    "chunking_method",
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
    "chunk_index",
    "content_hash",
    "chunk_hash",
    "chunk_text",
    "content_kind",
    "chunk_content_kind",
    "chunking_profile",
    "chunking_method",
    "chunking_fallback",
    "chunking_fallback_from",
    "preferred_chunking_method",
    "actual_chunking_method",
    "code_chunk_source",
    "markdown_block_kind",
    "section_level",
    "code_fence_language",
    "structured_record_kind",
    "toml_table",
    "transcript_speaker",
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
