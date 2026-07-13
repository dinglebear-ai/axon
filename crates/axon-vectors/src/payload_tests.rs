use axon_api::source::MetadataMap;
use serde_json::Value;

use crate::payload::{
    VECTOR_REDACTION_STATUS_VALUES, VECTOR_REQUIRED_FIELDS, VECTOR_VISIBILITY_VALUES,
    VectorPayload, VectorPayloadValidationError, source_family_allows_field,
};

fn fixture(name: &str) -> MetadataMap {
    let path = format!("tests/fixtures/payload/{name}");
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {path}: {err}"));
    let value: Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {path}: {err}"));
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{path} must be a JSON object"));
    let mut metadata = MetadataMap(
        object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    );
    apply_shared_lineage_fixture_defaults(&mut metadata);
    metadata
}

fn apply_shared_lineage_fixture_defaults(metadata: &mut MetadataMap) {
    // Fields added after these fixtures were authored (S2-18/S2-27):
    // distinct chunking profile/method + chunk_index. Backfilled here so
    // existing fixtures don't need per-file edits.
    metadata
        .entry("chunk_index".to_string())
        .or_insert_with(|| serde_json::json!(0));
    metadata
        .entry("chunking_profile".to_string())
        .or_insert_with(|| serde_json::json!("plain_text_windows"));
    metadata
        .entry("chunking_method".to_string())
        .or_insert_with(|| serde_json::json!("paragraph_windows"));

    let Some(source_family) = metadata
        .get("source_family")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    metadata
        .entry("source_kind".to_string())
        .or_insert_with(|| serde_json::json!(source_family));
    metadata
        .entry("source_adapter".to_string())
        .or_insert_with(|| serde_json::json!(source_family));
    metadata
        .entry("source_scope".to_string())
        .or_insert_with(|| serde_json::json!("item"));
    let canonical_uri = metadata
        .get("source_item_key")
        .cloned()
        .or_else(|| {
            metadata
                .get("chunk_locator")
                .and_then(|locator| locator.get("canonical_uri"))
                .cloned()
        })
        .unwrap_or_else(|| serde_json::json!("fixture://payload/item"));
    metadata
        .entry("source_item_key".to_string())
        .or_insert_with(|| canonical_uri.clone());
    metadata
        .entry("item_canonical_uri".to_string())
        .or_insert(canonical_uri);
}

#[test]
fn valid_payload_fixtures_pass_required_field_and_registry_validation() {
    for name in [
        "code.valid.json",
        "web.valid.json",
        "session.valid.json",
        "memory.valid.json",
        "package.valid.json",
    ] {
        let payload = VectorPayload::try_from_metadata(fixture(name))
            .unwrap_or_else(|err| panic!("{name} should validate: {err:?}"));
        for field in VECTOR_REQUIRED_FIELDS {
            assert!(
                payload.metadata().contains_key(*field),
                "{name} missing {field}"
            );
        }
        assert!(payload.metadata()["source_generation"].is_i64());
        assert!(
            payload.metadata()["committed_generation"].is_i64()
                || payload.metadata()["committed_generation"].is_null()
        );
    }
}

#[test]
fn initial_source_specific_registry_allows_only_declared_family_fields() {
    assert!(source_family_allows_field("code", "code_language"));
    assert!(source_family_allows_field("code", "code_symbol_name"));
    assert!(source_family_allows_field("code", "code_symbol_kind"));
    assert!(source_family_allows_field("code", "code_file_type"));
    assert!(source_family_allows_field("web", "web_title"));
    assert!(source_family_allows_field("web", "web_domain"));
    assert!(source_family_allows_field("web", "web_status_code"));
    assert!(source_family_allows_field("web", "web_depth"));
    assert!(source_family_allows_field("web", "normalization_version"));
    assert!(source_family_allows_field("web", "web_url"));
    assert!(source_family_allows_field("web", "web_seed_url"));
    assert!(source_family_allows_field("web", "web_origin"));
    assert!(source_family_allows_field("web", "web_path"));
    assert!(source_family_allows_field("web", "web_normalized_url"));
    assert!(source_family_allows_field("web", "web_fetch_method"));
    assert!(source_family_allows_field(
        "web",
        "structured_payload_omitted"
    ));
    assert!(source_family_allows_field("web", "web_structured_kind"));
    assert!(source_family_allows_field("web", "web_structured_blob"));
    assert!(source_family_allows_field("package", "package_ecosystem"));
    assert!(source_family_allows_field("package", "package_name"));
    assert!(source_family_allows_field("package", "package_version"));
    assert!(source_family_allows_field("session", "session_provider"));
    assert!(source_family_allows_field("session", "session_id"));
    assert!(source_family_allows_field("session", "session_turn_index"));
    assert!(source_family_allows_field("session", "session_tool_name"));
    assert!(source_family_allows_field("session", "session_skill_name"));
    assert!(source_family_allows_field("graph", "graph_node_ids"));
    assert!(source_family_allows_field("graph", "graph_edge_ids"));
    assert!(source_family_allows_field("graph", "graph_confidence"));
    assert!(source_family_allows_field("memory", "memory_id"));
    assert!(source_family_allows_field("memory", "memory_importance"));
    assert!(source_family_allows_field("memory", "memory_status"));

    assert!(!source_family_allows_field("web", "web_canonical_url"));
    assert!(!source_family_allows_field("code", "web_title"));
    assert!(!source_family_allows_field("code", "web_normalized_url"));
    // Structured-data projection fields (#298 dead-code recovery: the JSON-LD
    // / `__NEXT_DATA__` / SvelteKit payload projected onto web chunks by
    // `axon_document::preparer::project_structured_payload_metadata`) are
    // scoped to the `web` family only.
    assert!(!source_family_allows_field("code", "web_structured_kind"));
    assert!(!source_family_allows_field("code", "web_structured_blob"));
}

#[test]
fn source_family_registry_covers_phase_7_emitted_fields() {
    for (family, field) in [
        ("code", "code_language"),
        ("graph", "graph_node_ids"),
        ("local", "local_checkout"),
        ("tool", "tool_name"),
        ("tool", "tool_action"),
        ("tool", "tool_side_effect_class"),
        ("tool", "tool_output_artifact_id"),
        ("docker", "docker_image"),
        ("docker", "docker_service"),
        ("docker", "docker_port"),
        ("docker", "docker_volume"),
        ("env", "env_key"),
        ("env", "env_locator"),
    ] {
        assert!(
            source_family_allows_field(family, field),
            "missing metadata registry for {family}.{field}"
        );
    }
}

#[test]
fn invalid_payload_fixtures_report_the_expected_validation_error() {
    let cases = [
        (
            "secret.invalid.json",
            VectorPayloadValidationError::ForbiddenField {
                field: "raw_auth_headers".to_string(),
            },
        ),
        (
            "missing_chunk_text.invalid.json",
            VectorPayloadValidationError::MissingRequiredField {
                field: "chunk_text".to_string(),
            },
        ),
        (
            "missing_source_generation.invalid.json",
            VectorPayloadValidationError::MissingRequiredField {
                field: "source_generation".to_string(),
            },
        ),
        (
            "unknown_source_field.invalid.json",
            VectorPayloadValidationError::UnknownSourceSpecificField {
                field: "web_canonical_url".to_string(),
            },
        ),
        (
            "bad_visibility.invalid.json",
            VectorPayloadValidationError::InvalidVisibility,
        ),
        (
            "missing_source_family.invalid.json",
            VectorPayloadValidationError::MissingRequiredField {
                field: "source_family".to_string(),
            },
        ),
        (
            "forbidden_auth_header_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "web_title".to_string(),
            },
        ),
        (
            "forbidden_cookie_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "source_range.headers[0]".to_string(),
            },
        ),
        (
            "forbidden_dotenv_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "source_range.env[0]".to_string(),
            },
        ),
        (
            "forbidden_home_credential_path_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "chunk_locator.canonical_uri".to_string(),
            },
        ),
        (
            "forbidden_raw_html_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "web_title".to_string(),
            },
        ),
        (
            "forbidden_adapter_response_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "source_range.adapter_response".to_string(),
            },
        ),
        (
            "forbidden_bare_api_key_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "web_title".to_string(),
            },
        ),
        (
            "forbidden_embedded_bare_api_key_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "web_title".to_string(),
            },
        ),
        (
            "forbidden_absolute_home_path_value.invalid.json",
            VectorPayloadValidationError::ForbiddenValue {
                field: "chunk_locator.canonical_uri".to_string(),
            },
        ),
    ];

    for (name, expected) in cases {
        let err = VectorPayload::try_from_metadata(fixture(name)).unwrap_err();
        assert_eq!(err, expected, "{name}");
    }
}

#[test]
fn visibility_values_match_the_canonical_vector_payload_enum() {
    for visibility in VECTOR_VISIBILITY_VALUES {
        let mut metadata = fixture("web.valid.json");
        metadata.insert("visibility".to_string(), serde_json::json!(visibility));

        VectorPayload::try_from_metadata(metadata)
            .unwrap_or_else(|err| panic!("{visibility} should validate: {err:?}"));
    }

    let mut private = fixture("web.valid.json");
    private.insert("visibility".to_string(), serde_json::json!("private"));
    let err = VectorPayload::try_from_metadata(private).unwrap_err();
    assert_eq!(err, VectorPayloadValidationError::InvalidVisibility);
}

#[test]
fn redaction_status_values_match_the_canonical_vector_payload_enum() {
    for status in VECTOR_REDACTION_STATUS_VALUES {
        let mut metadata = fixture("web.valid.json");
        metadata.insert("redaction_status".to_string(), serde_json::json!(status));

        VectorPayload::try_from_metadata(metadata)
            .unwrap_or_else(|err| panic!("{status} should validate: {err:?}"));
    }

    let mut unknown = fixture("web.valid.json");
    unknown.insert("redaction_status".to_string(), serde_json::json!("unknown"));
    let err = VectorPayload::try_from_metadata(unknown).unwrap_err();
    assert_eq!(err, VectorPayloadValidationError::InvalidRedactionStatus);
}

#[test]
fn http_urls_with_local_path_words_do_not_trigger_local_path_redaction() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "chunk_locator".to_string(),
        serde_json::json!({
            "canonical_uri": "https://docs.example.com/users/home/setup",
            "path": "https://docs.example.com/users/home/setup",
            "heading_path": ["Users", "Home"],
            "symbol": null,
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );

    VectorPayload::try_from_metadata(metadata).unwrap();
}

#[test]
fn chunk_text_rejects_secret_like_body_values() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "chunk_text".to_string(),
        serde_json::json!("Use Authorization: Bearer secret-token in this request"),
    );

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(
        err,
        VectorPayloadValidationError::ForbiddenValue {
            field: "chunk_text".to_string()
        }
    );
}

#[test]
fn typed_payload_fields_reject_legacy_string_shapes() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "chunk_locator".to_string(),
        serde_json::json!("https://example.com/docs#intro"),
    );

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(
        err,
        VectorPayloadValidationError::InvalidFieldShape {
            field: "chunk_locator".to_string()
        }
    );
}

#[test]
fn typed_chunk_locator_values_reject_local_paths() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "chunk_locator".to_string(),
        serde_json::json!({
            "canonical_uri": "/tmp/axon/secret.rs",
            "path": "/tmp/axon/secret.rs",
            "heading_path": [],
            "symbol": null,
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(
        err,
        VectorPayloadValidationError::ForbiddenValue {
            field: "chunk_locator.canonical_uri".to_string()
        }
    );
}

#[test]
fn invalid_discriminators_are_not_echoed_in_error_messages() {
    let raw_visibility = "customer-alpha-supervalue-12345";
    let mut metadata = fixture("web.valid.json");
    metadata.insert("visibility".to_string(), serde_json::json!(raw_visibility));
    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();
    assert_eq!(err, VectorPayloadValidationError::InvalidVisibility);
    assert!(!err.to_string().contains(raw_visibility));

    let raw_family = "customer-alpha-family-12345";
    let mut metadata = fixture("web.valid.json");
    metadata.insert("source_family".to_string(), serde_json::json!(raw_family));
    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();
    assert_eq!(err, VectorPayloadValidationError::InvalidSourceFamily);
    assert!(!err.to_string().contains(raw_family));
}

#[test]
fn source_generation_fields_must_be_non_negative_integers() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert("source_generation".to_string(), serde_json::json!(""));

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(
        err,
        VectorPayloadValidationError::InvalidGeneration {
            field: "source_generation".to_string()
        }
    );
}

#[test]
fn payload_contract_version_must_match_current_contract() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "payload_contract_version".to_string(),
        serde_json::json!("2026-06-01"),
    );

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(err, VectorPayloadValidationError::InvalidContractVersion);
}

#[test]
fn source_ranges_must_preserve_start_end_order() {
    let mut metadata = fixture("web.valid.json");
    metadata.insert(
        "source_range".to_string(),
        serde_json::json!({
            "line_start": 10,
            "line_end": 2
        }),
    );

    let err = VectorPayload::try_from_metadata(metadata).unwrap_err();

    assert_eq!(
        err,
        VectorPayloadValidationError::InvalidFieldShape {
            field: "source_range.line_start_gt_end".to_string()
        }
    );
}

#[test]
fn typed_payload_fields_reject_incomplete_locator_and_empty_ranges() {
    let mut empty_range = fixture("web.valid.json");
    empty_range.insert("source_range".to_string(), serde_json::json!({}));
    let err = VectorPayload::try_from_metadata(empty_range).unwrap_err();
    assert_eq!(
        err,
        VectorPayloadValidationError::InvalidFieldShape {
            field: "source_range".to_string()
        }
    );

    let mut incomplete_locator = fixture("web.valid.json");
    incomplete_locator.insert(
        "chunk_locator".to_string(),
        serde_json::json!({
            "canonical_uri": "https://example.com/docs#intro",
            "path": "https://example.com/docs#intro",
            "heading_path": [],
            "symbol": null,
            "range": {}
        }),
    );
    let err = VectorPayload::try_from_metadata(incomplete_locator).unwrap_err();
    assert_eq!(
        err,
        VectorPayloadValidationError::InvalidFieldShape {
            field: "chunk_locator.range".to_string()
        }
    );
}
