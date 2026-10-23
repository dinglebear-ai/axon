//! Canonical code-search result and lineage mapping.
use super::*;

pub(super) fn target_vector_match_to_query_hit(
    rank: u64,
    hit: axon_api::source::VectorSearchMatch,
) -> Result<QueryHit, Box<dyn Error + Send + Sync>> {
    let source_range = hit.payload.get("source_range");
    let chunk_locator = hit.payload.get("chunk_locator");
    let canonical_uri = payload_string(&hit.payload, "item_canonical_uri")
        .or_else(|| payload_string(&hit.payload, "source_item_key"))
        .unwrap_or_else(|| hit.point_id.0.clone());
    let citation = code_search_citation(&hit, &canonical_uri)?;
    Ok(QueryHit {
        score_kind: Some(axon_api::result::QueryScoreKind::CodeSearch),
        rank,
        score: hit.score,
        rerank_score: hit.score,
        url: canonical_uri,
        source: payload_string(&hit.payload, "source_item_key").unwrap_or_default(),
        snippet: hit.text.unwrap_or_default(),
        citation,
        chunk_index: None,
        file_path: chunk_locator_path(chunk_locator)
            .or_else(|| payload_string(&hit.payload, "source_item_key")),
        symbol: chunk_locator
            .and_then(|value| value.get("symbol"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        kind: None,
        start_line: source_range_u32(source_range, "line_start"),
        end_line: source_range_u32(source_range, "line_end"),
        file_type: None,
        language: None,
        provider: payload_string(&hit.payload, "embedding_provider"),
        content_kind: payload_string(&hit.payload, "content_kind"),
        chunking_method: None,
        symbol_extraction_status: None,
    })
}

pub(super) fn code_search_citation(
    hit: &axon_api::source::VectorSearchMatch,
    canonical_uri: &str,
) -> Result<CanonicalCitation, Box<dyn Error + Send + Sync>> {
    let source_id = hit
        .source_id
        .clone()
        .ok_or_else(|| missing_lineage("source_id"))?;
    let source_item_key = hit
        .source_item_key
        .clone()
        .ok_or_else(|| missing_lineage("source_item_key"))?;
    let document_id = hit
        .document_id
        .clone()
        .ok_or_else(|| missing_lineage("document_id"))?;
    let chunk_id = hit
        .chunk_id
        .clone()
        .ok_or_else(|| missing_lineage("chunk_id"))?;
    let generation = hit
        .payload
        .get("source_generation")
        .and_then(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .or_else(|| value.as_i64().map(|value| value.to_string()))
        })
        .map(SourceGenerationId::new)
        .ok_or_else(|| missing_lineage("source_generation"))?;
    let job_id = payload_string(&hit.payload, "job_id")
        .and_then(|value| uuid::Uuid::parse_str(&value).ok())
        .map(JobId::new)
        .ok_or_else(|| missing_lineage("job_id"))?;
    let source_range = hit
        .payload
        .get("source_range")
        .cloned()
        .map(serde_json::from_value::<SourceRange>)
        .transpose()?
        .ok_or_else(|| missing_lineage("source_range"))?;
    let redaction = RedactionMetadata {
        redaction_status: payload_value(&hit.payload, "redaction_status")?,
        redaction_version: required_payload_string(&hit.payload, "redaction_version")?,
        visibility: payload_value(&hit.payload, "visibility")?,
        redacted_field_count: required_payload_u32(&hit.payload, "redacted_field_count")?,
        dropped_field_count: required_payload_u32(&hit.payload, "dropped_field_count")?,
        detector_count: required_payload_u32(&hit.payload, "detector_count")?,
        detector_names: payload_value(&hit.payload, "detector_names")?,
    };
    Ok(CanonicalCitation {
        source_id,
        source_item_key,
        generation,
        document_id,
        chunk_id,
        job_id,
        canonical_uri: canonical_uri.to_string(),
        source_range,
        redaction,
    })
}

pub(super) fn payload_value<T: serde::de::DeserializeOwned>(
    payload: &MetadataMap,
    field: &str,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    payload
        .get(field)
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or_else(|| missing_lineage(field))
}

pub(super) fn required_payload_string(
    payload: &MetadataMap,
    field: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    payload_string(payload, field).ok_or_else(|| missing_lineage(field))
}

pub(super) fn required_payload_u32(
    payload: &MetadataMap,
    field: &str,
) -> Result<u32, Box<dyn Error + Send + Sync>> {
    payload
        .get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| missing_lineage(field))
}

pub(super) fn missing_lineage(field: &str) -> Box<dyn Error + Send + Sync> {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("code search hit is missing canonical lineage field `{field}`"),
    )
    .into()
}

pub(super) fn payload_string(payload: &MetadataMap, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

pub(super) fn chunk_locator_path(locator: Option<&serde_json::Value>) -> Option<String> {
    locator?
        .get("path")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

pub(super) fn source_range_u32(range: Option<&serde_json::Value>, field: &str) -> Option<u32> {
    range?
        .get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
}
