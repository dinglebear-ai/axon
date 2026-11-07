//! Collection-spec helpers for vector stores.

use axon_api::source::*;

pub fn normalize_collection_spec(mut spec: CollectionSpec) -> CollectionSpec {
    for required in required_retrieval_payload_indexes() {
        if let Some(existing) = spec
            .payload_indexes
            .iter_mut()
            .find(|index| index.field_name == required.field_name)
        {
            // Reserved retrieval fields remain mandatory even when supplied as optional.
            // Type conflicts are rejected before normalization by store entry points.
            existing.required_for_filters = true;
        } else {
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
    let canonical = required_retrieval_payload_indexes();
    for index in &spec.payload_indexes {
        if let Some(required) = canonical
            .iter()
            .find(|required| required.field_name == index.field_name)
            && index.field_schema != required.field_schema
        {
            return Err(collection_drift(format!(
                "reserved payload index {} requires {:?}, received {:?}",
                required.field_name, required.field_schema, index.field_schema
            )));
        }
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
            // Missing indexes are repairable: Qdrant's index PUT is
            // idempotent, and ensure_collection runs it immediately after
            // drift validation. A present index with the wrong schema remains
            // fatal because Qdrant cannot change its type in place safely.
            continue;
        };
        if existing_index.field_schema != required.field_schema {
            let repair_hint = if matches!(
                required.field_name.as_str(),
                "source_generation" | "committed_generation"
            ) {
                "; verify stored generation payload values are integers, then rebuild only the affected payload index during maintenance while preserving collection points"
            } else {
                ""
            };
            return Err(collection_drift(format!(
                "collection {} payload index {} has a different field schema{}",
                existing.collection, required.field_name, repair_hint
            )));
        }
    }
    Ok(())
}

pub fn required_retrieval_payload_indexes() -> Vec<PayloadIndexSpec> {
    [
        ("source_id", PayloadFieldSchema::Keyword),
        ("source_family", PayloadFieldSchema::Keyword),
        ("source_kind", PayloadFieldSchema::Keyword),
        ("source_adapter", PayloadFieldSchema::Keyword),
        ("source_scope", PayloadFieldSchema::Keyword),
        ("source_canonical_uri", PayloadFieldSchema::Keyword),
        ("source_item_key", PayloadFieldSchema::Keyword),
        ("source_item_aliases", PayloadFieldSchema::Keyword),
        ("source_path_prefixes", PayloadFieldSchema::Keyword),
        ("item_canonical_uri_aliases", PayloadFieldSchema::Keyword),
        ("item_canonical_uri", PayloadFieldSchema::Keyword),
        ("source_generation", PayloadFieldSchema::Integer),
        ("committed_generation", PayloadFieldSchema::Integer),
        ("born_epoch", PayloadFieldSchema::Integer),
        ("retired_epoch", PayloadFieldSchema::Integer),
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
        ("embedded_at", PayloadFieldSchema::Datetime),
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
    ApiError::new("vector.collection_drift", ErrorStage::Upserting, message)
}

type Result<T> = std::result::Result<T, ApiError>;
