use super::system_requests::McpSystemRequest;
use super::{common::MCP_TOOL_SCHEMA_URI, server_authz};
use crate::schema::AxonRequest;
use axon_api::schema_registry::prune_public_job_kind_schemas;
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
    let mut schema = canonical_request_schema();
    enrich_tool_input_schema(&mut schema, &server_authz::mcp_action_names());
    schema
        .as_object()
        .expect("canonical MCP schema is an object")
        .clone()
}

/// Typed MCP request branches before the display-only aggregate field lift.
/// The dispatcher allowlist and request enum must agree in both directions.
pub(crate) fn canonical_request_schema() -> Value {
    static SCHEMA: LazyLock<Value> = LazyLock::new(build_canonical_request_schema);
    SCHEMA.clone()
}

fn build_canonical_request_schema() -> Value {
    let mut typed = serde_json::to_value(rmcp::schemars::schema_for!(AxonRequest))
        .expect("serialize canonical MCP request schema");
    let system = serde_json::to_value(rmcp::schemars::schema_for!(McpSystemRequest))
        .expect("serialize canonical system schema");
    typed["oneOf"]
        .as_array_mut()
        .expect("tagged MCP schema")
        .extend(
            system["oneOf"]
                .as_array()
                .expect("tagged system schema")
                .iter()
                .cloned(),
        );
    let supported = server_authz::mcp_action_names();
    let supported_set: HashSet<String> = supported.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        supported.len(),
        supported_set.len(),
        "duplicate runtime MCP action"
    );
    let typed_names = action_names_from_schema(&typed);
    assert_eq!(
        typed["oneOf"].as_array().expect("typed branches").len(),
        typed_names.len(),
        "duplicate typed MCP action"
    );
    assert_eq!(
        typed_names, supported_set,
        "typed MCP request / runtime action inventory drift"
    );

    let mut schema = json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "title": "AxonRequest", "oneOf": [], "$defs": {}});
    for spec in server_authz::MCP_ACTION_SPECS {
        let mut request = (spec.request_schema)();
        super::operation_schema::namespace_definitions(&mut request, spec.name);
        let object = request
            .as_object_mut()
            .expect("typed action request object");
        if let Some(definitions) = object.remove("$defs") {
            schema["$defs"]
                .as_object_mut()
                .expect("canonical definitions")
                .extend(definitions.as_object().expect("typed definitions").clone());
        }
        object.remove("$schema");
        object.remove("title");
        let fields = object
            .entry("properties")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("request fields");
        assert!(
            !fields.contains_key("action"),
            "request DTO owns reserved action field"
        );
        fields.insert(
            "action".to_owned(),
            json!({"type":"string", "const":spec.name}),
        );
        let required = object
            .entry("required")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("request requirements");
        required.push(json!("action"));
        if spec.name == "source" {
            required.push(json!("source"));
        }
        schema["oneOf"]
            .as_array_mut()
            .expect("canonical branches")
            .push(request);
    }
    sanitize_prune_schema(&mut schema);
    prune_public_job_kind_schemas(&mut schema);
    schema
}

fn enrich_tool_input_schema(schema: &mut Value, supported_actions: &[&'static str]) {
    let lifted_fields = collect_lifted_fields(schema);
    let subactions = axon_subaction_metadata(schema);
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
    object.insert("x-axon-subactions".to_string(), subactions);
    object.insert(
        "x-axon-agent-guidance".to_string(),
        json!({
            "cost_order": ["cheap", "moderate", "expensive", "write"],
            "first_pass": ["status", "doctor", "collections", "query", "retrieve", "help"],
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
        // The empty object preserves an unconstrained JSON schema while
        // supporting MCP clients that require object-shaped property schemas.
        if prop == Value::Bool(true) {
            prop = json!({});
        }
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

fn axon_subaction_metadata(schema: &Value) -> Value {
    let mut metadata = serde_json::Map::new();
    for branch in schema["oneOf"].as_array().expect("canonical branches") {
        let action = schema_branch_action(branch).expect("canonical action discriminator");
        let values = super::operation_schema::subactions(schema, branch);
        if !values.is_empty() {
            metadata.insert(action.to_owned(), json!(values));
        }
    }
    Value::Object(metadata)
}

fn sanitize_prune_schema(schema: &mut Value) {
    use super::system_requests::{ProvidersSubaction, PruneSubaction};
    let prune = serde_json::to_value(rmcp::schemars::schema_for!(PruneSubaction))
        .expect("prune enum schema");
    let providers = serde_json::to_value(rmcp::schemars::schema_for!(ProvidersSubaction))
        .expect("providers enum schema");
    for branch in schema["oneOf"].as_array_mut().expect("typed MCP branches") {
        if schema_branch_action(branch) == Some("prune") {
            branch["properties"]
                .as_object_mut()
                .expect("prune fields")
                .remove("collection");
        }
        let selector = match schema_branch_action(branch) {
            Some("prune") => Some(&prune),
            Some("providers") => Some(&providers),
            _ => None,
        };
        if let Some(selector) = selector {
            branch["properties"]["subaction"] = json!({ "anyOf": [selector, { "type": "null" }] });
        }
    }
}

fn axon_required_field_metadata() -> Value {
    json!({
        "source": ["source"]
    })
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
