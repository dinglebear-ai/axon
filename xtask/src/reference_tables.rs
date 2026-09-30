//! Deterministic human tables projected from generated registry records and DTOs.
use serde_json::Value;

fn cell(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Null => "not specified".to_owned(),
        _ => value.to_string(),
    };
    text.replace('&', "&amp;")
        .replace('|', "&#124;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "<br>")
}

fn table(records: &[Value], columns: &[&str]) -> String {
    let mut out = format!(
        "| {} |\n|{}|\n",
        columns.join(" | "),
        vec!["---"; columns.len()].join("|")
    );
    let mut records = records.iter().collect::<Vec<_>>();
    records.sort_by_key(|record| {
        columns
            .iter()
            .map(|key| cell(&record[*key]))
            .collect::<Vec<_>>()
    });
    for record in records {
        let cells = columns
            .iter()
            .map(|key| {
                if *key == "default" && record["secret"].as_bool() == Some(true) {
                    "redacted (secret setting)".to_owned()
                } else {
                    cell(&record[*key])
                }
            })
            .collect::<Vec<_>>();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

fn shape(schema: &Value) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or(reference).to_owned();
    }
    if let Some(kind) = schema.get("type") {
        return cell(kind);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            return format!(
                "{key}: {}",
                items.iter().map(shape).collect::<Vec<_>>().join(" / ")
            );
        }
    }
    match schema.as_bool() {
        Some(true) => "any JSON".to_owned(),
        Some(false) => "forbidden".to_owned(),
        None => "see linked JSON constraints".to_owned(),
    }
}

const CATALOGS: &[(&str, &str, &[&str])] = &[
    (
        "/commands",
        "Command Registry",
        &["name", "summary", "mutates", "async", "requires_auth_scope"],
    ),
    (
        "/routes",
        "Route Registry",
        &[
            "method",
            "path",
            "request_dto",
            "result_dto",
            "requires_auth_scope",
            "streaming",
        ],
    ),
    (
        "/config_keys",
        "Configuration Keys",
        &["key", "type", "default", "secret", "env_key", "description"],
    ),
    (
        "/env_vars",
        "Environment Variables",
        &[
            "name",
            "required",
            "default",
            "secret",
            "owner_crate",
            "description",
        ],
    ),
    (
        "/x-axon/operations",
        "MCP Operations",
        &[
            "name",
            "action",
            "subaction",
            "required_scope",
            "task_support",
        ],
    ),
];

pub(super) fn render(value: &Value) -> String {
    let mut out = String::new();
    for (pointer, title, columns) in CATALOGS {
        if let Some(records) = value.pointer(pointer).and_then(Value::as_array) {
            out.push_str(&format!(
                "\n## {title}\n\n{} records from the linked canonical artifact.\n\n",
                records.len()
            ));
            out.push_str(&table(records, columns));
        }
    }
    if let Some(definitions) = value
        .get("$defs")
        .or_else(|| value.get("definitions"))
        .and_then(Value::as_object)
    {
        out.push_str("\n## Definition Fields and Enums\n\nRequired means required by the linked JSON Schema; handler-specific conditional requirements and serialization omissions remain separate. References name definitions in that artifact. Full nested constraints remain in the JSON.\n");
        for (name, definition) in definitions {
            let properties = definition.get("properties").and_then(Value::as_object);
            let variants = definition.get("enum").and_then(Value::as_array);
            if properties.is_none() && variants.is_none() {
                continue;
            }
            out.push_str(&format!("\n### {}\n\n", cell(&Value::String(name.clone()))));
            if let Some(variants) = variants {
                out.push_str(&format!(
                    "Values: {}.\n",
                    variants.iter().map(cell).collect::<Vec<_>>().join(", ")
                ));
            }
            if let Some(properties) = properties {
                let required = definition.get("required").and_then(Value::as_array);
                out.push_str("| Field | Required | Shape | Description |\n|---|---|---|---|\n");
                for (field, schema) in properties {
                    let is_required = required
                        .is_some_and(|items| items.iter().any(|v| v.as_str() == Some(field)));
                    out.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        cell(&Value::String(field.clone())),
                        is_required,
                        shape(schema),
                        cell(&schema["description"])
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_real_registry_and_field_content_without_secret_defaults() {
        let doc = json!({"commands":[{"name":"crawl","summary":"a | b"}],"config_keys":[{"key":"auth.token","secret":true,"default":"synthetic-secret"}],"$defs":{"Request":{"type":"object","required":["query"],"properties":{"query":{"type":"string"},"limit":{"type":"integer"}}}}});
        let text = render(&doc);
        assert!(text.contains("crawl"));
        assert!(text.contains("a &#124; b"));
        assert!(text.contains("| query | true | string |"));
        assert!(text.contains("| limit | false | integer |"));
        assert!(!text.contains("synthetic-secret"));
        assert_eq!(text, render(&doc));
    }
}
