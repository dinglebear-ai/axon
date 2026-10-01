use super::*;
use serde_json::json;

#[test]
fn generated_chunk_alias_and_recovery_fields_accept_runtime_shapes() {
    let root = fixture_repo();
    generate(root.path()).unwrap();
    let schema = generated_json(
        root.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let validator = validator_for(&schema).unwrap();
    let mut payload = schema["x-axon"]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|payload| payload["source_family"] == "code")
        .unwrap()
        .clone();
    for (field, value) in [
        ("source_item_aliases", json!(["copied/lib.rs"])),
        (
            "item_canonical_uri_aliases",
            json!(["https://example.com/copied/lib.rs"]),
        ),
        (
            "source_path_prefixes",
            json!(["/", "copied", "copied/lib.rs"]),
        ),
        ("code_symbol_aliases", json!(["first", "second"])),
        ("code_syntax_recovered", json!(true)),
        ("code_symbol_source_range", payload["source_range"].clone()),
    ] {
        payload[field] = value;
    }
    assert!(validator.is_valid(&payload));
    let runtime =
        axon_api::source::MetadataMap(payload.as_object().unwrap().clone().into_iter().collect());
    axon_vectors::payload::VectorPayload::try_from_metadata(runtime).unwrap();
    for field in [
        "source_item_aliases",
        "item_canonical_uri_aliases",
        "source_path_prefixes",
        "code_symbol_aliases",
    ] {
        let mut invalid = payload.clone();
        invalid[field] = json!([" \t"]);
        assert!(!validator.is_valid(&invalid), "whitespace {field}");
    }
    for field in [
        "source_item_aliases",
        "source_path_prefixes",
        "code_symbol_aliases",
        "code_syntax_recovered",
        "code_symbol_source_range",
    ] {
        let mut invalid = payload.clone();
        invalid[field] = json!("wrong shape");
        assert!(!validator.is_valid(&invalid), "{field}");
    }
}
