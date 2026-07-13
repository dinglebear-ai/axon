//! Field-shape validation helpers for `VectorPayload`.
//!
//! Split out of `payload.rs` to keep that file under the repo's 500-line
//! monolith cap. Pure shape/range checks; no field-presence or
//! forbidden-value policy (those stay in `payload.rs`).

use axon_api::source::{ChunkLocator, MetadataMap, SourceRange};

use crate::payload::VectorPayloadValidationError;

pub(crate) fn validate_shapes(metadata: &MetadataMap) -> Result<(), VectorPayloadValidationError> {
    for field in [
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
        // `chunking_profile`/`chunking_method` are distinct fields (S2-27,
        // S2-18): the profile the router selected vs. the concrete method
        // actually used. Neither should be conflated with `embedding_profile`
        // (an embedding-pipeline identity, e.g. "document" vs. a future
        // "query" profile), which used to be repurposed to carry the
        // chunking profile string.
        "chunking_profile",
        "chunking_method",
    ] {
        require_non_empty_string(metadata, field)?;
    }
    require_positive_integer(metadata, "embedding_dimensions")?;
    require_non_negative_integer(metadata, "chunk_index")?;

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

fn require_non_negative_integer(
    metadata: &MetadataMap,
    field: &str,
) -> Result<(), VectorPayloadValidationError> {
    if metadata
        .get(field)
        .and_then(|value| value.as_i64())
        .is_some_and(|value| value >= 0)
    {
        Ok(())
    } else {
        Err(VectorPayloadValidationError::InvalidFieldShape {
            field: field.to_string(),
        })
    }
}
