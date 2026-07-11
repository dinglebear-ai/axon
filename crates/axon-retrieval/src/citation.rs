//! Citation helpers for the retrieval boundary fake.

use axon_api::source::{ChunkId, DocumentId, SourceId, SourceRange, VectorSearchMatch};
use axon_error::{ApiError, ErrorStage};
use serde_json::Value;

pub const MODULE_NAME: &str = "citation";

#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
    pub source_id: SourceId,
    pub document_id: DocumentId,
    pub chunk_id: ChunkId,
    pub canonical_uri: String,
    pub range: SourceRange,
}

impl Citation {
    pub(crate) fn new(
        source_id: SourceId,
        document_id: DocumentId,
        chunk_id: ChunkId,
        canonical_uri: String,
        range: SourceRange,
    ) -> Self {
        Self {
            source_id,
            document_id,
            chunk_id,
            canonical_uri,
            range,
        }
    }

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
            document_id,
            chunk_id,
            canonical_uri,
            range,
        })
    }
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
