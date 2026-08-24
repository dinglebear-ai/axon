use std::error::Error;

use axon_api::source::{MetadataMap, SourceGenerationId, SourceId, VectorSearchRequest};
use axon_vectors::payload::generation_payload_i64;
use serde_json::json;

pub(super) fn target_code_search_request(
    collection: String,
    query: &str,
    limit: usize,
    dense_vector: Vec<f32>,
    source_id: &SourceId,
    committed_generation: &SourceGenerationId,
    path_prefix: Option<&str>,
    language: Option<&str>,
) -> Result<VectorSearchRequest, Box<dyn Error + Send + Sync>> {
    let mut filters = MetadataMap::new();
    filters.insert("source_id".to_string(), json!(source_id.0));
    filters.insert(
        "committed_generation".to_string(),
        json!(generation_payload_i64(
            committed_generation,
            "committed_generation"
        )?),
    );
    filters.insert("visibility".to_string(), json!("internal"));
    filters.insert("redaction_status".to_string(), json!("clean"));
    if let Some(prefix) = path_prefix {
        filters.insert("path_prefix".to_string(), json!(prefix));
    }
    if let Some(language) = language {
        filters.insert("language".to_string(), json!(language));
    }
    Ok(VectorSearchRequest {
        collection,
        query: query.to_string(),
        limit: u32::try_from(limit).unwrap_or(u32::MAX),
        dense_vector: Some(dense_vector),
        sparse_vector: None,
        filters,
        hybrid: Some(false),
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    })
}
