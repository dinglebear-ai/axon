//! Citation helpers for the retrieval boundary fake.

use axon_api::source::{
    ChunkId, DocumentId, JobId, RedactionMetadata, RedactionStatus, SourceGenerationId, SourceId,
    SourceItemKey, SourceRange, VectorSearchMatch, Visibility,
};
use axon_error::{ApiError, ErrorStage};
use serde_json::Value;

pub const MODULE_NAME: &str = "citation";

#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub document_id: DocumentId,
    pub chunk_id: ChunkId,
    pub job_id: JobId,
    pub canonical_uri: String,
    pub range: SourceRange,
    pub redaction: RedactionMetadata,
}

impl Citation {
    pub(crate) fn from_vector_match(item: &VectorSearchMatch) -> Result<Self, ApiError> {
        let chunk_id = item.chunk_id.clone().ok_or_else(|| {
            ApiError::new(
                "retrieval.missing_chunk_id",
                ErrorStage::Retrieving,
                "vector match is missing chunk_id",
            )
        })?;
        let document_id = item.document_id.clone().ok_or_else(|| {
            ApiError::new(
                "retrieval.missing_document_id",
                ErrorStage::Retrieving,
                format!("vector match {} is missing document_id", item.point_id.0),
            )
        })?;
        let source_id = item.source_id.clone().ok_or_else(|| {
            ApiError::new(
                "retrieval.missing_source_id",
                ErrorStage::Retrieving,
                format!("vector match {} is missing source_id", item.point_id.0),
            )
        })?;
        let source_item_key = item
            .source_item_key
            .clone()
            .ok_or_else(|| missing_metadata(item, "source_item_key"))?;
        let generation = required_generation(item)?;
        let job_id = required_job_id(item)?;
        let redaction = required_redaction(item)?;
        let canonical_uri = item
            .payload
            .get("chunk_locator")
            .and_then(|value| value.as_object())
            .and_then(|value| value.get("canonical_uri"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ApiError::new(
                    "retrieval.missing_canonical_uri",
                    ErrorStage::Retrieving,
                    format!("vector match {} is missing canonical_uri", item.point_id.0),
                )
            })?
            .to_string();
        let range = payload_range(item)?;
        if !has_locator(&range) {
            return Err(ApiError::new(
                "retrieval.missing_source_range",
                ErrorStage::Retrieving,
                format!(
                    "vector match {} is missing source range locator fields",
                    item.point_id.0
                ),
            ));
        }

        Ok(Self {
            source_id,
            source_item_key,
            generation,
            document_id,
            chunk_id,
            job_id,
            canonical_uri,
            range,
            redaction,
        })
    }
}

fn required_generation(item: &VectorSearchMatch) -> Result<SourceGenerationId, ApiError> {
    item.payload
        .get("source_generation")
        .and_then(Value::as_i64)
        .filter(|generation| *generation >= 0)
        .map(|generation| SourceGenerationId::new(generation.to_string()))
        .ok_or_else(|| missing_metadata(item, "source_generation"))
}

fn required_job_id(item: &VectorSearchMatch) -> Result<JobId, ApiError> {
    item.payload
        .get("job_id")
        .and_then(Value::as_str)
        .and_then(|job_id| uuid::Uuid::parse_str(job_id).ok())
        .map(JobId::new)
        .ok_or_else(|| missing_metadata(item, "job_id"))
}

fn required_redaction(item: &VectorSearchMatch) -> Result<RedactionMetadata, ApiError> {
    let status = item
        .payload
        .get("redaction_status")
        .cloned()
        .and_then(|value| serde_json::from_value::<RedactionStatus>(value).ok())
        .ok_or_else(|| missing_metadata(item, "redaction_status"))?;
    let visibility = item
        .payload
        .get("visibility")
        .cloned()
        .and_then(|value| serde_json::from_value::<Visibility>(value).ok())
        .ok_or_else(|| missing_metadata(item, "visibility"))?;
    let version = required_string(item, "redaction_version")?;
    Ok(RedactionMetadata {
        redaction_status: status,
        redaction_version: version,
        visibility,
        redacted_field_count: required_u32(item, "redacted_field_count")?,
        dropped_field_count: required_u32(item, "dropped_field_count")?,
        detector_count: required_u32(item, "detector_count")?,
        detector_names: item
            .payload
            .get("detector_names")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|_| missing_metadata(item, "detector_names"))?
            .ok_or_else(|| missing_metadata(item, "detector_names"))?,
    })
}

fn required_string(item: &VectorSearchMatch, field: &str) -> Result<String, ApiError> {
    item.payload
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| missing_metadata(item, field))
}

fn required_u32(item: &VectorSearchMatch, field: &str) -> Result<u32, ApiError> {
    item.payload
        .get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| missing_metadata(item, field))
}

fn missing_metadata(item: &VectorSearchMatch, field: &str) -> ApiError {
    ApiError::new(
        format!("retrieval.missing_{field}"),
        ErrorStage::Retrieving,
        format!("vector match {} is missing valid {field}", item.point_id.0),
    )
}

fn payload_range(item: &VectorSearchMatch) -> Result<SourceRange, ApiError> {
    let source_range = source_range_from_payload(item, "source_range")?;
    let chunk_locator_range = item
        .payload
        .get("chunk_locator")
        .and_then(|value| value.as_object())
        .and_then(|value| value.get("range"))
        .map(|value| source_range_from_value(value, "retrieval.invalid_chunk_locator_range", item))
        .transpose()?;

    Ok(merge_ranges(source_range, chunk_locator_range))
}

fn source_range_from_payload(
    item: &VectorSearchMatch,
    field: &str,
) -> Result<Option<SourceRange>, ApiError> {
    item.payload
        .get(field)
        .map(|value| source_range_from_value(value, "retrieval.invalid_source_range", item))
        .transpose()
}

fn source_range_from_value(
    value: &Value,
    code: &str,
    item: &VectorSearchMatch,
) -> Result<SourceRange, ApiError> {
    serde_json::from_value(value.clone()).map_err(|err| {
        ApiError::new(
            code,
            ErrorStage::Retrieving,
            format!(
                "vector match {} has malformed source range: {err}",
                item.point_id.0
            ),
        )
    })
}

fn merge_ranges(source: Option<SourceRange>, locator: Option<SourceRange>) -> SourceRange {
    let source = source.unwrap_or_else(empty_range);
    let locator = locator.unwrap_or_else(empty_range);
    SourceRange {
        line_start: source.line_start.or(locator.line_start),
        line_end: source.line_end.or(locator.line_end),
        byte_start: source.byte_start.or(locator.byte_start),
        byte_end: source.byte_end.or(locator.byte_end),
        char_start: source.char_start.or(locator.char_start),
        char_end: source.char_end.or(locator.char_end),
        time_start_ms: source.time_start_ms.or(locator.time_start_ms),
        time_end_ms: source.time_end_ms.or(locator.time_end_ms),
        dom_selector: non_empty(source.dom_selector).or_else(|| non_empty(locator.dom_selector)),
        json_pointer: non_empty(source.json_pointer).or_else(|| non_empty(locator.json_pointer)),
        yaml_path: non_empty(source.yaml_path).or_else(|| non_empty(locator.yaml_path)),
        xml_xpath: non_empty(source.xml_xpath).or_else(|| non_empty(locator.xml_xpath)),
        csv_row: source.csv_row.or(locator.csv_row),
        session_turn_id: non_empty(source.session_turn_id)
            .or_else(|| non_empty(locator.session_turn_id)),
        turn_start: non_empty(source.turn_start).or_else(|| non_empty(locator.turn_start)),
        turn_end: non_empty(source.turn_end).or_else(|| non_empty(locator.turn_end)),
    }
}

fn empty_range() -> SourceRange {
    SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn has_locator(range: &SourceRange) -> bool {
    range.line_start.is_some()
        || range.line_end.is_some()
        || range.byte_start.is_some()
        || range.byte_end.is_some()
        || range.char_start.is_some()
        || range.char_end.is_some()
        || range.time_start_ms.is_some()
        || range.time_end_ms.is_some()
        || range.dom_selector.is_some()
        || range.json_pointer.is_some()
        || range.yaml_path.is_some()
        || range.xml_xpath.is_some()
        || range.csv_row.is_some()
        || range.session_turn_id.is_some()
        || range.turn_start.is_some()
        || range.turn_end.is_some()
}
