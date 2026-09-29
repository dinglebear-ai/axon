use super::*;
use crate::server::{operations::operation_registry, tool_schema};

#[test]
fn every_schema_is_closed_and_fixed_discriminators_are_absent() {
    for op in operation_registry() {
        let schema = Value::Object(op.input_schema.as_ref().clone());
        jsonschema::validator_for(&schema).unwrap_or_else(|error| panic!("{}: {error}", op.name));
        let properties = schema["properties"].as_object().unwrap();
        assert!(!properties.contains_key("action"), "{}", op.name);
        if op.subaction.is_some() {
            assert!(!properties.contains_key("subaction"), "{}", op.name);
        }
        let mut pending = BTreeSet::new();
        references(&schema, &mut pending);
        for name in pending {
            assert!(schema["$defs"].get(&name).is_some(), "{}: {name}", op.name);
        }
    }
}

#[test]
fn focused_schema_has_only_reachable_definitions_and_preserves_unions() {
    let root = tool_schema::canonical_request_schema();
    let query = Value::Object(focused(&root, "query", None));
    assert!(query["$defs"].as_object().unwrap().len() < root["$defs"].as_object().unwrap().len());
    assert!(query["properties"]["response_mode"].get("anyOf").is_some());
    assert!(
        !query["properties"]
            .as_object()
            .unwrap()
            .contains_key("source")
    );
    let validator = jsonschema::validator_for(&query).unwrap();
    assert!(validator.is_valid(&json!({"query":"schema preservation", "limit":3})));
    assert!(!validator.is_valid(&json!({"query":17})));
    assert!(!validator.is_valid(&json!({"action":"query", "query":"x"})));
    assert!(!validator.is_valid(&json!({"query":"x", "limit":-1})));
}

#[test]
fn discriminator_specialization_preserves_conditional_and_nested_payload_semantics() {
    let root = json!({"oneOf":[{
        "type":"object", "additionalProperties":false,
        "properties":{"action":{"const":"family"}, "subaction":{"enum":["read","write"]}, "id":{"type":"string"}, "payload":{"type":"object","properties":{"action":{"type":"string"}}}},
        "required":["action","subaction"],
        "allOf":[{"if":{"properties":{"subaction":{"const":"write"}},"required":["subaction"]},"then":{"required":["id"]}}]
    }]});
    let write = Value::Object(focused(&root, "family", Some("write")));
    let read = Value::Object(focused(&root, "family", Some("read")));
    let write_validator = jsonschema::validator_for(&write).unwrap();
    let read_validator = jsonschema::validator_for(&read).unwrap();
    assert!(!write_validator.is_valid(&json!({})));
    assert!(write_validator.is_valid(&json!({"id":"owned","payload":{"action":"user-data"}})));
    assert!(read_validator.is_valid(&json!({})));
    assert!(!read_validator.is_valid(&json!({"subaction":"write"})));
}

#[test]
#[should_panic(expected = "no canonical schema")]
fn missing_action_schema_never_falls_back_to_empty() {
    focused(&json!({"oneOf":[]}), "missing", None);
}

#[test]
#[should_panic(expected = "missing reachable")]
fn unresolved_reference_fails_construction() {
    focused(
        &json!({"oneOf":[{"properties":{"action":{"const":"bad"},"x":{"$ref":"#/$defs/Missing"}}}]}),
        "bad",
        None,
    );
}

#[test]
fn canonical_and_specialized_validity_agree_for_representative_payloads() {
    let canonical = tool_schema::canonical_request_schema();
    let validator = jsonschema::validator_for(&canonical).unwrap();
    let samples = [
        json!({}),
        json!({"query":"owned evidence", "limit":3}),
        json!({"source":"https://example.com", "response_mode":"inline"}),
        json!({"job_id":"11111111-1111-4111-8111-111111111111"}),
        json!({"id":"owned-resource", "limit":5}),
        json!({"query":42}),
        json!({"limit":-3}),
        json!({"unexpected_fixed_sibling":true}),
    ];
    for operation in operation_registry() {
        let projected =
            jsonschema::validator_for(&Value::Object(operation.input_schema.as_ref().clone()))
                .unwrap();
        for sample in &samples {
            let mut routed = sample.clone();
            routed["action"] = json!(operation.action);
            if let Some(subaction) = &operation.subaction {
                routed["subaction"] = json!(subaction);
            }
            assert_eq!(
                projected.is_valid(sample),
                validator.is_valid(&routed),
                "{}: {sample}",
                operation.name
            );
        }
    }
}
