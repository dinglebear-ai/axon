//! Qdrant collection spec for memory vector points.

use axon_api::source::*;

use super::{MEMORY_COLLECTION_ALIAS, MemoryVectorConfig};

pub(super) fn memory_collection_spec(config: &MemoryVectorConfig) -> CollectionSpec {
    CollectionSpec {
        collection: config.collection.clone(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: config.embedding_dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: memory_payload_indexes(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: vec![MEMORY_COLLECTION_ALIAS.to_string()],
        metadata: MetadataMap::new(),
        distance: Some(VectorDistance::Cosine),
    }
}

pub(super) fn memory_payload_indexes() -> Vec<PayloadIndexSpec> {
    [
        ("vector_namespace", PayloadFieldSchema::Keyword),
        ("memory_id", PayloadFieldSchema::Keyword),
        ("memory_type", PayloadFieldSchema::Keyword),
        ("memory_status", PayloadFieldSchema::Keyword),
        ("memory_scope_kind", PayloadFieldSchema::Keyword),
        ("memory_scope_value", PayloadFieldSchema::Keyword),
        ("redaction_status", PayloadFieldSchema::Keyword),
        ("visibility", PayloadFieldSchema::Keyword),
    ]
    .into_iter()
    .map(|(field_name, field_schema)| PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema,
        required_for_filters: true,
    })
    .collect()
}
