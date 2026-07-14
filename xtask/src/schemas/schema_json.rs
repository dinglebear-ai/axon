use anyhow::Result;
use axon_api::schema_registry::prune_public_job_kind_schemas;
use serde_json::{Map, Value};

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
