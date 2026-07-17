use super::system_requests::{
    ArtifactsSubaction, CollectionsSubaction, McpSystemRequest, ResetSubaction, UploadsSubaction,
    WatchMcpRequest,
};
use super::{common::MCP_TOOL_SCHEMA_URI, server_authz};
use crate::schema::{AxonRequest, ExtractSubaction, JobsSubaction, MemorySubaction};
use axon_api::schema_registry::prune_public_job_kind_schemas;
use rmcp::schemars::JsonSchema;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, LazyLock};

pub(super) fn axon_tool_input_schema() -> Arc<rmcp::model::JsonObject> {
    static SCHEMA: LazyLock<Arc<rmcp::model::JsonObject>> =
        LazyLock::new(|| Arc::new(build_axon_tool_input_schema()));
    Arc::clone(&SCHEMA)
}

pub(super) fn mcp_tool_schema_markdown() -> String {
    let schema_json =
        serde_json::to_string_pretty(&Value::Object(axon_tool_input_schema().as_ref().clone()))
            .unwrap_or_else(|_| "{}".to_string());
    format!(
        "# Axon MCP Tool Schema\n\nURI: `{}`\n\nSingle tool name: `axon`\n\nRouting contract:\n- `action` is required\n- `subaction` selects an operation within subaction families; many families default it when omitted\n- `response_mode` supports `path|inline|both|auto_inline`; most actions default to `path`, while `retrieve` defaults to inline paged document reads\n\n## JSON Schema\n\n```json\n{}\n```\n",
        MCP_TOOL_SCHEMA_URI, schema_json
    )
}

fn build_axon_tool_input_schema() -> rmcp::model::JsonObject {
    let mut schema =
        serde_json::to_value(rmcp::schemars::schema_for!(AxonRequest)).unwrap_or_else(|_| {
            json!({
                "type": "object",
                "properties": {},
            })
        });
    append_system_request_branches(&mut schema);
    replace_watch_request_schema(&mut schema);
    sanitize_prune_schema(&mut schema);

    let supported_actions = server_authz::mcp_action_names();
    let supported_set: HashSet<&str> = supported_actions.iter().copied().collect();
    let typed_actions = action_names_from_schema(&schema);
    for action in &supported_actions {
        debug_assert!(
            typed_actions.contains(*action),
            "MCP action spec `{action}` has no matching AxonRequest schema variant"
        );
    }
    filter_schema_to_supported_actions(&mut schema, &supported_set);
    prune_public_job_kind_schemas(&mut schema);
    enrich_tool_input_schema(&mut schema, &supported_actions);

    match schema {
        Value::Object(object) => object,
        _ => serde_json::Map::new(),
    }
}

fn enrich_tool_input_schema(schema: &mut Value, supported_actions: &[&'static str]) {
    let lifted_fields = collect_lifted_fields(schema);
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.insert("type".to_string(), json!("object"));
    object.insert("required".to_string(), json!(["action"]));
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| json!({}));
    let Some(properties) = properties.as_object_mut() else {
        return;
    };
    properties.insert(
        "action".to_string(),
        json!({
            "type": "string",
            "enum": supported_actions,
            "description": "Action to run. The enum is derived from Axon's MCP action specs, which also drive scope checks."
        }),
    );
    properties.insert(
        "subaction".to_string(),
        json!({
            "type": "string",
            "description": "Operation within a subaction family. See x-axon-subactions for valid values by action."
        }),
    );
    insert_lifted_fields(properties, lifted_fields);
    object.insert("x-axon-action-metadata".to_string(), axon_action_metadata());
    object.insert(
        "x-axon-required-fields".to_string(),
        axon_required_field_metadata(),
    );
    object.insert("x-axon-subactions".to_string(), axon_subaction_metadata());
    object.insert(
        "x-axon-agent-guidance".to_string(),
        json!({
            "cost_order": ["cheap", "moderate", "expensive", "write"],
            "first_pass": ["status", "doctor", "sources", "domains", "stats", "query", "retrieve", "help"],
            "index_with": ["source"],
            "async_jobs": ["extract"],
            "poll_async_jobs_with": {
                "action": "jobs",
                "subaction": "get",
                "required_field": "job_id"
            },
            "artifact_first": {
                "default_response_mode": "path",
                "inline_defaults": ["retrieve"]
            },
            "schema_resource": MCP_TOOL_SCHEMA_URI
        }),
    );
}

/// Per-action request fields harvested from the `oneOf` branches so they can
/// be republished as an optional superset in top-level `properties`.
///
/// Many MCP clients (Codex, mcporter signatures, Labby's codemode `.d.ts`
/// surface consumers) render a tool's callable parameters from top-level
/// `properties` only and ignore `oneOf` — without this lift they see just
/// `{action, subaction}`. Per-action requirements and `additionalProperties`
/// strictness stay in the untouched `oneOf` branches; serde enforcement in
/// `parse_axon_request` is unaffected.
struct LiftedField {
    /// Distinct field shapes across branches, descriptions stripped.
    variants: Vec<Value>,
    /// First non-empty description encountered across branches.
    description: Option<String>,
    /// Actions whose branch declares this field.
    actions: BTreeSet<String>,
}

fn collect_lifted_fields(schema: &Value) -> BTreeMap<String, LiftedField> {
    let mut fields: BTreeMap<String, LiftedField> = BTreeMap::new();
    let Some(branches) = schema.get("oneOf").and_then(Value::as_array) else {
        return fields;
    };
    for branch in branches {
        let Some(action) = schema_branch_action(branch) else {
            continue;
        };
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (name, prop) in properties {
            // `action` and `subaction` keep their injected top-level forms.
            if name == "action" || name == "subaction" {
                continue;
            }
            let mut stripped = prop.clone();
            let description = stripped
                .as_object_mut()
                .and_then(|object| object.remove("description"))
                .and_then(|value| value.as_str().map(str::to_string))
                .filter(|text| !text.is_empty());
            let entry = fields.entry(name.clone()).or_insert_with(|| LiftedField {
                variants: Vec::new(),
                description: None,
                actions: BTreeSet::new(),
            });
            entry.actions.insert(action.to_string());
            if entry.description.is_none() {
                entry.description = description;
            }
            if !entry.variants.contains(&stripped) {
                entry.variants.push(stripped);
            }
        }
    }
    fields
}

fn insert_lifted_fields(
    properties: &mut serde_json::Map<String, Value>,
    lifted_fields: BTreeMap<String, LiftedField>,
) {
    for (name, field) in lifted_fields {
        if properties.contains_key(&name) {
            continue;
        }
        let mut prop = match <[Value; 1]>::try_from(field.variants) {
            Ok([only]) => only,
            Err(variants) => json!({ "anyOf": variants }),
        };
        if let Some(object) = prop.as_object_mut() {
            let actions: Vec<&str> = field.actions.iter().map(String::as_str).collect();
            let prefix = format!("Applies to action(s): {}.", actions.join(", "));
            let description = match &field.description {
                Some(text) => format!("{prefix} {text}"),
                None => prefix,
            };
            object.insert("description".to_string(), json!(description));
            object.insert("x-axon-actions".to_string(), json!(actions));
        }
        properties.insert(name, prop);
    }
}

fn axon_action_metadata() -> Value {
    Value::Array(
        server_authz::MCP_ACTION_SPECS
            .iter()
            .map(|spec| {
                json!({
                    "name": spec.name,
                    "scope": spec.scope.as_label(),
                    "cost": spec.cost,
                    "description": spec.description,
                })
            })
            .collect(),
    )
}

fn axon_subaction_metadata() -> Value {
    json!({
        "extract": enum_values_for::<ExtractSubaction>(),
        "jobs": enum_values_for::<JobsSubaction>(),
        "memory": enum_values_for::<MemorySubaction>(),
        "prune": ["plan", "exec"],
        "collections": enum_values_for::<CollectionsSubaction>(),
        "reset": enum_values_for::<ResetSubaction>(),
        "uploads": enum_values_for::<UploadsSubaction>(),
        "artifacts": enum_values_for::<ArtifactsSubaction>(),
    })
}

fn append_system_request_branches(schema: &mut Value) {
    let generated =
        serde_json::to_value(rmcp::schemars::schema_for!(McpSystemRequest)).unwrap_or(Value::Null);
    let Some(branches) = generated.get("oneOf").and_then(Value::as_array) else {
        return;
    };
    if let Some(target) = schema.get_mut("oneOf").and_then(Value::as_array_mut) {
        target.extend(branches.iter().cloned());
    }
    let Some(definitions) = generated.get("$defs").and_then(Value::as_object) else {
        return;
    };
    if let Some(target) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        target.extend(definitions.clone());
    }
}

fn replace_watch_request_schema(schema: &mut Value) {
    let generated =
        serde_json::to_value(rmcp::schemars::schema_for!(WatchMcpRequest)).unwrap_or(Value::Null);
    let Some(watch) = generated
        .get("$defs")
        .and_then(|defs| defs.get("WatchMcpRequest"))
    else {
        return;
    };
    if let Some(definitions) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        definitions.insert("WatchRequest".to_string(), watch.clone());
        if let Some(extra) = generated.get("$defs").and_then(Value::as_object) {
            for (name, value) in extra {
                if name != "WatchMcpRequest" {
                    definitions.insert(name.clone(), value.clone());
                }
            }
        }
    }
}

fn sanitize_prune_schema(schema: &mut Value) {
    let Some(prune) = schema.pointer_mut("/$defs/PruneMcpRequest") else {
        return;
    };
    if let Some(object) = prune.as_object_mut() {
        object.insert(
            "description".to_string(),
            json!("Canonical cleanup planning and execution. Only plan and exec are public subactions."),
        );
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            properties.remove("collection");
            if let Some(subaction) = properties
                .get_mut("subaction")
                .and_then(Value::as_object_mut)
            {
                subaction.insert(
                    "description".to_string(),
                    json!("plan or exec; defaults to plan"),
                );
            }
        }
    }
}

fn axon_required_field_metadata() -> Value {
    json!({
        "source": ["source"]
    })
}

fn enum_values_for<T>() -> Vec<String>
where
    T: JsonSchema,
{
    let schema = serde_json::to_value(rmcp::schemars::schema_for!(T)).unwrap_or(Value::Null);
    let mut values = Vec::new();
    collect_string_enums(&schema, &mut values);
    values.sort();
    values.dedup();
    values
}

fn action_names_from_schema(schema: &Value) -> HashSet<String> {
    let mut actions = Vec::new();
    collect_action_names(schema, &mut actions);
    actions.into_iter().collect()
}

fn collect_action_names(value: &Value, out: &mut Vec<String>) {
    if let Some(action) = value.pointer("/properties/action") {
        collect_string_enums(action, out);
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_action_names(item, out);
            }
        }
        Value::Object(object) => {
            for key in ["oneOf", "anyOf", "allOf"] {
                if let Some(values) = object.get(key).and_then(Value::as_array) {
                    for item in values {
                        collect_action_names(item, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_string_enums(value: &Value, out: &mut Vec<String>) {
    if let Some(value) = value.get("const").and_then(Value::as_str) {
        out.push(value.to_string());
    }
    if let Some(values) = value.get("enum").and_then(Value::as_array) {
        out.extend(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string),
        );
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_string_enums(item, out);
            }
        }
        Value::Object(object) => {
            for key in ["oneOf", "anyOf", "allOf"] {
                if let Some(values) = object.get(key).and_then(Value::as_array) {
                    for item in values {
                        collect_string_enums(item, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn filter_schema_to_supported_actions(schema: &mut Value, supported_actions: &HashSet<&str>) {
    match schema {
        Value::Array(items) => {
            for item in items {
                filter_schema_to_supported_actions(item, supported_actions);
            }
        }
        Value::Object(object) => {
            for key in ["oneOf", "anyOf"] {
                if let Some(values) = object.get_mut(key).and_then(Value::as_array_mut) {
                    values.retain(|item| {
                        schema_branch_action(item)
                            .is_none_or(|action| supported_actions.contains(action))
                    });
                    for item in values {
                        filter_schema_to_supported_actions(item, supported_actions);
                    }
                }
            }
            if let Some(values) = object.get_mut("allOf").and_then(Value::as_array_mut) {
                for item in values {
                    filter_schema_to_supported_actions(item, supported_actions);
                }
            }
        }
        _ => {}
    }
}

fn schema_branch_action(value: &Value) -> Option<&str> {
    let action = value.pointer("/properties/action")?;
    action.get("const").and_then(Value::as_str).or_else(|| {
        action
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
    })
}
