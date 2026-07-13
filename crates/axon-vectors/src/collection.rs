//! Collection-spec helpers for vector stores.

use axon_api::source::*;

pub fn normalize_collection_spec(mut spec: CollectionSpec) -> CollectionSpec {
    for required in required_retrieval_payload_indexes() {
        if !spec
            .payload_indexes
            .iter()
            .any(|index| index.field_name == required.field_name)
        {
            spec.payload_indexes.push(required);
        }
    }
    spec.payload_indexes
        .sort_by(|left, right| left.field_name.cmp(&right.field_name));
    spec.payload_indexes
        .dedup_by(|left, right| left.field_name == right.field_name);
    spec.aliases.sort();
    spec.aliases.dedup();
    spec
}

pub fn validate_collection_spec(spec: &CollectionSpec) -> Result<()> {
    if spec.collection.trim().is_empty() {
        return Err(collection_drift(
            "collection name must be non-empty".to_string(),
        ));
    }
    if spec.dense.name.trim().is_empty() {
        return Err(collection_drift(
            "dense vector name must be non-empty".to_string(),
        ));
    }
    if spec.dense.dimensions == 0 {
        return Err(collection_drift(
            "dense vector dimensions must be greater than zero".to_string(),
        ));
    }
    if let Some(sparse) = &spec.sparse
        && sparse.name.trim().is_empty()
    {
        return Err(collection_drift(
            "sparse vector name must be non-empty".to_string(),
        ));
    }
    Ok(())
}

pub fn check_collection_drift(existing: &CollectionSpec, incoming: &CollectionSpec) -> Result<()> {
    if existing.dense != incoming.dense || existing.sparse != incoming.sparse {
        return Err(collection_drift(format!(
            "collection {} already exists with a different vector configuration",
            existing.collection
        )));
    }
    for required in incoming
        .payload_indexes
        .iter()
        .filter(|index| index.required_for_filters)
    {
        let Some(existing_index) = existing
            .payload_indexes
            .iter()
            .find(|index| index.field_name == required.field_name)
        else {
            return Err(collection_drift(format!(
                "collection {} is missing required payload index {}",
                existing.collection, required.field_name
            )));
        };
        if existing_index.field_schema != required.field_schema {
            let reset_hint = if matches!(
                required.field_name.as_str(),
                "source_generation" | "committed_generation"
            ) {
                "; generation payload index schema changed for the clean-break cutover, run preflight/reset before reusing this collection"
            } else {
                ""
            };
            return Err(collection_drift(format!(
                "collection {} payload index {} has a different field schema{}",
                existing.collection, required.field_name, reset_hint
            )));
        }
    }
    Ok(())
}

pub fn required_retrieval_payload_indexes() -> Vec<PayloadIndexSpec> {
    [
        ("source_id", PayloadFieldSchema::Keyword),
        ("source_kind", PayloadFieldSchema::Keyword),
        ("source_adapter", PayloadFieldSchema::Keyword),
        ("source_scope", PayloadFieldSchema::Keyword),
        ("source_canonical_uri", PayloadFieldSchema::Keyword),
        ("source_item_key", PayloadFieldSchema::Keyword),
        ("item_canonical_uri", PayloadFieldSchema::Keyword),
        ("source_generation", PayloadFieldSchema::Integer),
        ("committed_generation", PayloadFieldSchema::Integer),
        ("document_id", PayloadFieldSchema::Keyword),
        ("chunk_id", PayloadFieldSchema::Keyword),
        ("job_id", PayloadFieldSchema::Keyword),
        ("vector_namespace", PayloadFieldSchema::Keyword),
        ("visibility", PayloadFieldSchema::Keyword),
        ("redaction_status", PayloadFieldSchema::Keyword),
        ("document_status", PayloadFieldSchema::Keyword),
        ("content_kind", PayloadFieldSchema::Keyword),
        ("embedding_provider", PayloadFieldSchema::Keyword),
        ("embedding_model", PayloadFieldSchema::Keyword),
        ("embedding_profile", PayloadFieldSchema::Keyword),
        ("web_domain", PayloadFieldSchema::Keyword),
    ]
    .into_iter()
    .map(|(field_name, field_schema)| PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema,
        required_for_filters: true,
    })
    .collect()
}

fn collection_drift(message: String) -> ApiError {
    ApiError::new(
        "vector.collection_drift",
        axon_error::ErrorStage::Upserting,
        message,
    )
}

type Result<T> = std::result::Result<T, ApiError>;
