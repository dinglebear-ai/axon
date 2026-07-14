use anyhow::Result;
use serde_json::{Map, Value, json};

pub(super) fn json_string(value: &Value) -> Result<String> {
    let mut content = serde_json::to_string_pretty(value)?;
    content.push('\n');
    Ok(content)
}

pub(super) fn schema_defs(schemas: &[(&str, Value)], enums: Option<Value>) -> Value {
    let mut defs = Map::new();
    for (name, schema) in schemas {
        add_schema_def(&mut defs, name, schema.clone());
    }
    if let Some(enums) = enums {
        defs.insert("enums".to_string(), enums);
    }
    Value::Object(defs)
}

fn add_schema_def(defs: &mut Map<String, Value>, name: &str, mut schema: Value) {
    let inner_defs = schema
        .as_object_mut()
        .and_then(|object| object.remove("$defs"))
        .and_then(|defs| defs.as_object().cloned())
        .unwrap_or_default();
    rewrite_refs(&mut schema, name);
    prune_public_job_kind_schemas(&mut schema);
    defs.insert(name.to_string(), schema);

    for (inner_name, mut inner_schema) in inner_defs {
        rewrite_refs(&mut inner_schema, name);
        prune_public_job_kind_schemas(&mut inner_schema);
        defs.insert(format!("{name}_{inner_name}"), inner_schema);
    }
}

fn rewrite_refs(value: &mut Value, namespace: &str) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref")
                && let Some(rest) = reference.strip_prefix("#/$defs/")
            {
                *reference = format!("#/$defs/{namespace}_{rest}");
            }
            for value in map.values_mut() {
                rewrite_refs(value, namespace);
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_refs(value, namespace);
            }
        }
        _ => {}
    }
}

fn prune_public_job_kind_schemas(value: &mut Value) {
    let public_values = public_job_kind_values();
    let all_values = all_job_kind_values();
    prune_public_job_kind_schemas_inner(value, &public_values, &all_values);
}

fn prune_public_job_kind_schemas_inner(
    value: &mut Value,
    public_values: &[String],
    all_values: &[String],
) {
    if is_job_kind_schema(value, all_values) {
        *value = json!({
            "type": "string",
            "enum": public_values,
        });
        return;
    }

    match value {
        Value::Object(map) => {
            for value in map.values_mut() {
                prune_public_job_kind_schemas_inner(value, public_values, all_values);
            }
        }
        Value::Array(values) => {
            for value in values {
                prune_public_job_kind_schemas_inner(value, public_values, all_values);
            }
        }
        _ => {}
    }
}

fn is_job_kind_schema(value: &Value, all_values: &[String]) -> bool {
    let mut values = Vec::new();
    collect_schema_string_values(value, &mut values);
    if values.len() < 4 {
        return false;
    }
    values.iter().all(|value| all_values.contains(value))
        && values.iter().any(|value| value == "source")
        && values.iter().any(|value| value == "provider_probe")
}

fn collect_schema_string_values(value: &Value, out: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        out.extend(
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string)),
        );
    }
    if let Some(value) = object.get("const").and_then(Value::as_str) {
        out.push(value.to_string());
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(values) = object.get(key).and_then(Value::as_array) {
            for value in values {
                collect_schema_string_values(value, out);
            }
        }
    }
}

fn all_job_kind_values() -> Vec<String> {
    axon_api::source::JobKind::all()
        .iter()
        .copied()
        .map(job_kind_wire_value)
        .collect()
}

fn public_job_kind_values() -> Vec<String> {
    axon_api::source::JobKind::all()
        .iter()
        .copied()
        .filter(|kind| kind.is_public_source_surface())
        .map(job_kind_wire_value)
        .collect()
}

fn job_kind_wire_value(kind: axon_api::source::JobKind) -> String {
    serde_json::to_value(kind)
        .expect("JobKind serializes")
        .as_str()
        .expect("JobKind serializes to string")
        .to_string()
}
