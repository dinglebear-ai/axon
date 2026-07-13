use axon_api::source::{
    CollectionSpec, MetadataMap, PayloadFieldSchema, PayloadIndexSpec, VectorConfig, VectorDistance,
};

use crate::collection::{
    check_collection_drift, normalize_collection_spec, required_retrieval_payload_indexes,
};

#[test]
fn required_retrieval_payload_indexes_include_generation_safe_filters() {
    let indexes = required_retrieval_payload_indexes();
    let required = [
        "source_id",
        "source_kind",
        "source_adapter",
        "source_scope",
        "source_canonical_uri",
        "source_item_key",
        "item_canonical_uri",
        "source_generation",
        "committed_generation",
        "document_id",
        "chunk_id",
        "job_id",
        "vector_namespace",
        "visibility",
        "redaction_status",
        "document_status",
        "content_kind",
        "embedding_provider",
        "embedding_model",
        "embedding_profile",
        "web_domain",
    ];

    for field_name in required {
        let index = indexes
            .iter()
            .find(|index| index.field_name == field_name)
            .unwrap_or_else(|| panic!("missing required payload index {field_name}"));
        let expected_schema = match field_name {
            "source_generation" | "committed_generation" => PayloadFieldSchema::Integer,
            _ => PayloadFieldSchema::Keyword,
        };
        assert_eq!(index.field_schema, expected_schema);
        assert!(
            index.required_for_filters,
            "{field_name} must be marked required for filters"
        );
    }
    for legacy_field in [
        "url",
        "seed_url",
        "domain",
        "source_type",
        "payload_schema_version",
    ] {
        assert!(
            !indexes.iter().any(|index| index.field_name == legacy_field),
            "legacy field {legacy_field} must not be in target retrieval index profile"
        );
    }
}

#[test]
fn keyword_generation_index_drift_requires_clean_break_reset() {
    let mut existing = normalize_collection_spec(CollectionSpec {
        collection: "axon".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 768,
            distance: VectorDistance::Cosine,
        },
        sparse: None,
        payload_indexes: vec![PayloadIndexSpec {
            field_name: "source_generation".to_string(),
            field_schema: PayloadFieldSchema::Keyword,
            required_for_filters: true,
        }],
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    });
    let incoming = normalize_collection_spec(CollectionSpec {
        collection: "axon".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 768,
            distance: VectorDistance::Cosine,
        },
        sparse: None,
        payload_indexes: Vec::new(),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    });
    existing
        .payload_indexes
        .retain(|index| index.field_name != "source_generation");
    existing.payload_indexes.push(PayloadIndexSpec {
        field_name: "source_generation".to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    });

    let err = check_collection_drift(&existing, &incoming).expect_err("generation index drift");

    assert!(err.message.contains("clean-break cutover"));
    assert!(err.message.contains("preflight/reset"));
}
