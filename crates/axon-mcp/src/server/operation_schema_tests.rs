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
fn finite_leaves_expose_only_fields_used_by_their_operation() {
    for op in operation_registry() {
        let Some(selector) = op.subaction.as_deref() else {
            continue;
        };
        let Some(fields) = leaf_fields(op.action, selector) else {
            continue;
        };
        let actual: BTreeSet<_> = op.input_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(actual, fields.iter().copied().collect(), "{}", op.name);
    }
    for (name, irrelevant) in [
        ("jobs_get", "retry_mode"),
        ("jobs_list", "confirm"),
        ("memory_show", "import_mode"),
        ("watch_get", "every_seconds"),
        ("uploads_get", "content"),
        ("graph_node", "edges"),
        ("codex_snapshot", "operation_id"),
        ("prune_get", "confirm"),
        ("reset_get", "stores"),
    ] {
        let schema = Value::Object(
            operation_registry()
                .iter()
                .find(|op| op.name == name)
                .unwrap()
                .input_schema
                .as_ref()
                .clone(),
        );
        assert!(
            !schema["properties"]
                .as_object()
                .unwrap()
                .contains_key(irrelevant),
            "{name}"
        );
    }
    for (name, required_field) in [
        ("jobs_get", "job_id"),
        ("memory_show", "id"),
        ("uploads_create", "filename"),
        ("watch_create", "source"),
        ("prune_exec", "confirm"),
        ("jobs_recover", "stale_before"),
        ("jobs_clear", "confirm"),
        ("codex_prepare", "params"),
        ("codex_execute", "params"),
    ] {
        let schema = Value::Object(
            operation_registry()
                .iter()
                .find(|op| op.name == name)
                .unwrap()
                .input_schema
                .as_ref()
                .clone(),
        );
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(!validator.is_valid(&json!({})), "{name}");
        let null_field = Value::Object(Map::from_iter([(required_field.to_owned(), Value::Null)]));
        assert!(!validator.is_valid(&null_field), "{name}");
        if schema["properties"][required_field]["type"] == "string" {
            let blank = Value::Object(Map::from_iter([(required_field.to_owned(), json!("  \t"))]));
            assert!(!validator.is_valid(&blank), "{name}");
        }
    }
    let exec = Value::Object(
        operation_registry()
            .iter()
            .find(|op| op.name == "prune_exec")
            .unwrap()
            .input_schema
            .as_ref()
            .clone(),
    );
    assert!(
        !jsonschema::validator_for(&exec)
            .unwrap()
            .is_valid(&json!({"plan_id":"owned", "confirm":false}))
    );
    for (name, invalid, valid) in [
        (
            "memory_show",
            json!({"id": " \t"}),
            json!({"id": "memory-1"}),
        ),
        (
            "prune_plan",
            json!({"target": " \t"}),
            json!({"target": "all"}),
        ),
        (
            "reset_get",
            json!({"plan_id": " \t"}),
            json!({"plan_id": "plan-1"}),
        ),
        (
            "codex_events",
            json!({"cursor_boot_id": 1}),
            json!({"cursor_boot_id": 1, "after_sequence": 2}),
        ),
        ("codex_events", json!({"after_sequence": 2}), json!({})),
        (
            "codex_events",
            json!({"cursor_boot_id": null, "after_sequence": 2}),
            json!({"cursor_boot_id": 1, "after_sequence": 2}),
        ),
        (
            "codex_reconcile",
            json!({"operation_id": 1, "without_replay": true}),
            json!({"operation_id": 1, "without_replay": true, "effect_applied": false, "disposition_note": "checked"}),
        ),
        (
            "codex_reconcile",
            json!({"operation_id": 1, "without_replay": true, "effect_applied": null, "disposition_note": null}),
            json!({"operation_id": 1, "without_replay": true, "effect_applied": false, "disposition_note": "checked"}),
        ),
    ] {
        let schema = Value::Object(
            operation_registry()
                .iter()
                .find(|op| op.name == name)
                .unwrap()
                .input_schema
                .as_ref()
                .clone(),
        );
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(!validator.is_valid(&invalid), "{name}: {invalid}");
        assert!(validator.is_valid(&valid), "{name}: {valid}");
    }
    for (name, field) in [
        ("graph_query", "node_id"),
        ("graph_resolve", "canonical_uri"),
        ("watch_history", "source"),
        ("uploads_put_content", "content"),
    ] {
        let schema = Value::Object(
            operation_registry()
                .iter()
                .find(|op| op.name == name)
                .unwrap()
                .input_schema
                .as_ref()
                .clone(),
        );
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(!validator.is_valid(&json!({})), "{name}");
        let mut null_choice = Map::new();
        null_choice.insert(field.to_owned(), Value::Null);
        if name == "uploads_put_content" {
            null_choice.insert("upload_id".to_owned(), json!("owned"));
        }
        assert!(!validator.is_valid(&Value::Object(null_choice)), "{name}");
        let mut valid = Map::new();
        valid.insert(field.to_owned(), json!("owned"));
        if name == "uploads_put_content" {
            valid.insert("upload_id".to_owned(), json!("owned"));
        }
        assert!(validator.is_valid(&Value::Object(valid)), "{name}");
    }
}

#[test]
fn every_flat_family_field_is_classified_or_explicitly_unused() {
    let root = tool_schema::canonical_request_schema();
    for action in [
        "artifacts",
        "codex",
        "collections",
        "graph",
        "jobs",
        "memory",
        "providers",
        "prune",
        "reset",
        "uploads",
        "watch",
    ] {
        let branch = action_branch(&root, action);
        let declared: BTreeSet<_> = branch["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|field| !matches!(*field, "action" | "subaction"))
            .collect();
        let classified: BTreeSet<_> = subactions(&root, branch)
            .iter()
            .flat_map(|selector| leaf_fields(action, selector).unwrap().iter().copied())
            .collect();
        let unused: BTreeSet<_> = declared.difference(&classified).copied().collect();
        let expected: BTreeSet<_> = match action {
            "memory" => ["depth", "response_mode", "salience"].into(),
            "reset" => ["include_config", "reason"].into(),
            _ => BTreeSet::new(),
        };
        assert_eq!(
            unused, expected,
            "{action}: update leaf field classification when the DTO changes"
        );
    }
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
            let focused = projected.is_valid(sample);
            let canonical = validator.is_valid(&routed);
            assert!(!focused || canonical, "{}: {sample}", operation.name);
            if operation.subaction.is_none() {
                assert_eq!(focused, canonical, "{}: {sample}", operation.name);
            }
        }
    }
}
