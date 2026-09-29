//! Schema specialization preserves applicators instead of merging their properties.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn action_branch<'a>(root: &'a Value, action: &str) -> &'a Value {
    let branches = root["oneOf"]
        .as_array()
        .expect("canonical MCP tagged union");
    let mut matches = branches.iter().filter(|branch| {
        branch
            .pointer("/properties/action/const")
            .and_then(Value::as_str)
            == Some(action)
    });
    let branch = matches
        .next()
        .unwrap_or_else(|| panic!("no canonical schema for {action}"));
    assert!(
        matches.next().is_none(),
        "duplicate canonical branch for {action}"
    );
    branch
}

pub(crate) fn subactions(root: &Value, branch: &Value) -> Vec<String> {
    let Some(selector) = branch.pointer("/properties/subaction") else {
        return Vec::new();
    };
    let mut values = BTreeSet::new();
    assert!(
        collect_strings(root, selector, &mut values),
        "non-finite MCP selector needs explicit route metadata: {selector}"
    );
    assert!(
        !values.is_empty(),
        "subaction schema has no executable string values"
    );
    values.into_iter().collect()
}

fn collect_strings(root: &Value, schema: &Value, values: &mut BTreeSet<String>) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return collect_strings(root, resolve(root, reference), values);
    }
    if let Some(value) = schema.get("const").and_then(Value::as_str) {
        values.insert(value.to_owned());
        return true;
    }
    if let Some(items) = schema.get("enum").and_then(Value::as_array) {
        for item in items {
            if let Some(value) = item.as_str() {
                values.insert(value.to_owned());
            } else if !item.is_null() {
                return false;
            }
        }
        return true;
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            return items.iter().all(|item| collect_strings(root, item, values));
        }
    }
    schema.get("type").is_some_and(|kind| kind == "null")
}

fn resolve<'a>(root: &'a Value, reference: &str) -> &'a Value {
    let pointer = reference
        .strip_prefix('#')
        .expect("MCP schemas must use local references");
    root.pointer(pointer)
        .unwrap_or_else(|| panic!("unresolved MCP schema reference {reference}"))
}

/// Remove fixed selectors at this instance level, never inside a user payload.
pub(crate) fn focused(root: &Value, action: &str, subaction: Option<&str>) -> Map<String, Value> {
    let mut fixed = BTreeMap::from([("action", action)]);
    if let Some(subaction) = subaction {
        fixed.insert("subaction", subaction);
    }
    let schema = specialize(root, action_branch(root, action), &fixed);
    let mut object = schema
        .as_object()
        .expect("an operation must have an object schema")
        .clone();
    object.insert("type".into(), json!("object"));
    object.entry("properties").or_insert_with(|| json!({}));
    close_definitions(root, &mut object);
    object
}

fn literal_matches(root: &Value, schema: &Value, literal: &str) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return literal_matches(root, resolve(root, reference), literal);
    }
    if let Some(value) = schema.get("const") {
        return value == literal;
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.iter().any(|value| value == literal);
    }
    if let Some(values) = schema.get("anyOf").and_then(Value::as_array) {
        return values
            .iter()
            .any(|value| literal_matches(root, value, literal));
    }
    if let Some(values) = schema.get("oneOf").and_then(Value::as_array) {
        return values
            .iter()
            .filter(|value| literal_matches(root, value, literal))
            .count()
            == 1;
    }
    if let Some(values) = schema.get("allOf").and_then(Value::as_array) {
        return values
            .iter()
            .all(|value| literal_matches(root, value, literal));
    }
    schema.get("type").is_none_or(|kind| {
        kind == "string"
            || kind
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "string"))
    })
}

fn specialize(root: &Value, schema: &Value, fixed: &BTreeMap<&str, &str>) -> Value {
    let Some(mut object) = schema.as_object().cloned() else {
        return schema.clone();
    };
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for (field, literal) in fixed {
            if let Some(constraint) = properties.remove(*field)
                && !literal_matches(root, &constraint, literal)
            {
                return json!(false);
            }
        }
        if properties.is_empty() {
            object.remove("properties");
        }
    }
    if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|field| {
            !field
                .as_str()
                .is_some_and(|field| fixed.contains_key(field))
        });
        if required.is_empty() {
            object.remove("required");
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
            for item in items.iter_mut() {
                *item = specialize(root, item, fixed);
            }
            if key == "allOf" && items.iter().any(|item| item == &json!(false)) {
                return json!(false);
            }
            if key != "allOf" {
                items.retain(|item| item != &json!(false));
            }
            if items.is_empty() && key != "allOf" {
                return json!(false);
            }
        }
    }
    for key in ["if", "then", "else", "not"] {
        if let Some(item) = object.get_mut(key) {
            *item = specialize(root, item, fixed);
        }
    }
    if let Some(reference) = object
        .remove("$ref")
        .and_then(|value| value.as_str().map(str::to_owned))
    {
        let resolved = specialize(root, resolve(root, &reference), fixed);
        object
            .entry("allOf")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("allOf array")
            .push(resolved);
    }
    Value::Object(object)
}

fn references(value: &Value, pending: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let name = reference
                    .strip_prefix("#/$defs/")
                    .expect("MCP schema reference outside $defs");
                assert!(
                    !name.contains('/'),
                    "MCP reference must name a complete definition"
                );
                pending.insert(name.replace("~1", "/").replace("~0", "~"));
            }
            for (key, value) in object {
                if !matches!(
                    key.as_str(),
                    "$defs" | "examples" | "default" | "const" | "enum"
                ) {
                    references(value, pending);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                references(item, pending);
            }
        }
        _ => {}
    }
}

fn close_definitions(root: &Value, schema: &mut Map<String, Value>) {
    schema.remove("$defs");
    let mut pending = BTreeSet::new();
    let mut definitions = Map::new();
    references(&Value::Object(schema.clone()), &mut pending);
    while let Some(name) = pending.pop_first() {
        if definitions.contains_key(&name) {
            continue;
        }
        let definition = root
            .get("$defs")
            .and_then(|defs| defs.get(&name))
            .unwrap_or_else(|| panic!("missing reachable MCP definition {name}"));
        references(definition, &mut pending);
        definitions.insert(name, definition.clone());
    }
    if !definitions.is_empty() {
        schema.insert("$defs".into(), Value::Object(definitions));
    }
}

/// Namespace independently derived schemas before composing their definitions.
pub(crate) fn namespace_definitions(schema: &mut Value, prefix: &str) {
    fn rewrite(value: &mut Value, prefix: &str) {
        match value {
            Value::Object(object) => {
                if let Some(Value::String(reference)) = object.get_mut("$ref")
                    && let Some(name) = reference.strip_prefix("#/$defs/")
                {
                    *reference = format!("#/$defs/{prefix}_{name}");
                }
                for (key, value) in object {
                    if !matches!(key.as_str(), "examples" | "default" | "const" | "enum") {
                        rewrite(value, prefix);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    rewrite(item, prefix);
                }
            }
            _ => {}
        }
    }
    rewrite(schema, prefix);
    if let Some(definitions) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        *definitions = std::mem::take(definitions)
            .into_iter()
            .map(|(name, value)| (format!("{prefix}_{name}"), value))
            .collect();
    }
}

#[cfg(test)]
#[path = "operation_schema_tests.rs"]
mod tests;
