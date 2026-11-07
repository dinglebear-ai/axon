use axon_api::source::{
    CollectionSpec, MetadataMap, PayloadFieldSchema, PayloadIndexSpec, VectorConfig, VectorDistance,
};

use crate::collection::{
    check_collection_drift, normalize_collection_spec, required_retrieval_payload_indexes,
};

#[test]
fn required_retrieval_payload_indexes_match_target_profile() {
    let indexes = required_retrieval_payload_indexes();
    let required = [
        "source_id",
        "source_family",
        "source_kind",
        "source_adapter",
        "source_scope",
        "source_canonical_uri",
        "source_item_key",
        "source_item_aliases",
        "source_path_prefixes",
        "item_canonical_uri_aliases",
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
        "embedded_at",
        "web_domain",
    ];

    for field_name in required {
        let index = indexes
            .iter()
            .find(|index| index.field_name == field_name)
            .unwrap_or_else(|| panic!("missing required payload index {field_name}"));
        let expected_schema = match field_name {
            "source_generation" | "committed_generation" => PayloadFieldSchema::Integer,
            "embedded_at" => PayloadFieldSchema::Datetime,
            _ => PayloadFieldSchema::Keyword,
        };
        assert_eq!(index.field_schema, expected_schema);
        assert!(
            index.required_for_filters,
            "{field_name} must be marked required for filters"
        );
    }
}

#[test]
fn target_index_profile_excludes_legacy_fields() {
    let indexes = required_retrieval_payload_indexes();
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
fn missing_required_payload_index_is_repairable_drift() {
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
    let mut existing = incoming.clone();
    existing
        .payload_indexes
        .retain(|index| index.field_name != "born_epoch");

    check_collection_drift(&existing, &incoming)
        .expect("ensure_collection can recreate a missing payload index");
}

#[test]
fn keyword_generation_index_drift_requires_index_only_repair() {
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

    assert!(
        err.message
            .contains("verify stored generation payload values are integers")
    );
    assert!(
        err.message
            .contains("rebuild only the affected payload index")
    );
    assert!(err.message.contains("preserving collection points"));
    assert!(!err.message.contains("preflight/reset"));
}

#[test]
fn reserved_payload_index_types_cannot_be_overridden_or_marked_optional() {
    for required_for_filters in [true, false] {
        let mut spec = crate::testing::test_collection_spec(3);
        spec.payload_indexes = vec![PayloadIndexSpec {
            field_name: "source_generation".into(),
            field_schema: PayloadFieldSchema::Keyword,
            required_for_filters,
        }];
        let error = crate::collection::validate_collection_spec(&spec)
            .expect_err("reserved integer payload field cannot use keyword index");
        assert_eq!(error.code.to_string(), "vector.collection_drift");
        assert!(error.message.contains("source_generation"));
    }
}

#[test]
fn canonical_payload_index_cannot_be_marked_optional() {
    let mut spec = crate::testing::test_collection_spec(3);
    spec.payload_indexes
        .iter_mut()
        .find(|index| index.field_name == "source_generation")
        .unwrap()
        .required_for_filters = false;
    let normalized = normalize_collection_spec(spec);
    assert!(
        normalized
            .payload_indexes
            .iter()
            .find(|index| index.field_name == "source_generation")
            .unwrap()
            .required_for_filters
    );
}

#[test]
fn conflicting_duplicate_reserved_index_is_rejected_before_normalization() {
    let mut spec = crate::testing::test_collection_spec(3);
    spec.payload_indexes.push(PayloadIndexSpec {
        field_name: "source_generation".into(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: false,
    });
    assert!(crate::qdrant::qdrant_collection_request(&spec).is_err());
}
