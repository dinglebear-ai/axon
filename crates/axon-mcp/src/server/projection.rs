use super::{AxonMcpServer, server_authz, tool_schema};
use axon_core::config::McpProjection;
use rmcp::{
    ErrorData,
    model::{CallToolRequestParams, Tool, ToolAnnotations},
};
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub(super) const ATOMIC_TOOL_PREFIX: &str = "axon_";

pub(super) fn atomic_tool_name(action: &str) -> String {
    format!("{ATOMIC_TOOL_PREFIX}{action}")
}

pub(super) fn atomic_action_from_tool_name(name: &str) -> Option<&'static str> {
    let action = name.strip_prefix(ATOMIC_TOOL_PREFIX)?;
    server_authz::MCP_ACTION_SPECS
        .iter()
        .find(|spec| spec.name == action)
        .map(|spec| spec.name)
}

pub(super) fn normalize_projected_tool_call(
    server: &AxonMcpServer,
    request: &mut CallToolRequestParams,
) -> Result<(), ErrorData> {
    let projection = server.cfg.mcp_projection;

    if request.name.as_ref() == "axon" {
        if projection_allows_legacy(projection) {
            return Ok(());
        }
        return Err(super::common::invalid_params(
            "legacy `axon` tool is disabled by AXON_MCP_PROJECTION",
        ));
    }

    if AxonMcpServer::tool_router()
        .get(request.name.as_ref())
        .is_some()
    {
        return Ok(());
    }

    if !projection_allows_atomic(projection) {
        return Err(super::common::invalid_params(
            "atomic MCP tools are disabled by AXON_MCP_PROJECTION",
        ));
    }

    let Some(action) = atomic_action_from_tool_name(request.name.as_ref()) else {
        return Err(super::common::invalid_params(format!(
            "unknown MCP tool `{}`",
            request.name
        )));
    };

    let arguments = request.arguments.get_or_insert_with(Map::new);
    if let Some(existing) = arguments.get("action").and_then(Value::as_str)
        && existing != action
    {
        return Err(super::common::invalid_params(format!(
            "atomic tool `{}` cannot override action with `{existing}`",
            request.name
        )));
    }
    arguments.insert("action".to_string(), Value::String(action.to_string()));
    request.name = "axon".into();
    Ok(())
}

pub(super) fn projection_allows_legacy(projection: McpProjection) -> bool {
    matches!(projection, McpProjection::Legacy | McpProjection::Both)
}

pub(super) fn projection_allows_atomic(projection: McpProjection) -> bool {
    matches!(projection, McpProjection::Atomic | McpProjection::Both)
}

pub(super) fn projected_tools(projection: McpProjection, router_tools: Vec<Tool>) -> Vec<Tool> {
    let legacy = router_tools
        .iter()
        .find(|tool| tool.name == "axon")
        .cloned();
    let mut tools = router_tools
        .into_iter()
        .filter(|tool| tool.name != "axon")
        .collect::<Vec<_>>();

    if projection_allows_legacy(projection)
        && let Some(legacy) = legacy
    {
        tools.push(legacy);
    }
    if projection_allows_atomic(projection) {
        tools.extend(
            server_authz::MCP_ACTION_SPECS
                .iter()
                .map(atomic_tool_for_spec),
        );
    }
    tools
}

pub(super) fn projected_tool(
    projection: McpProjection,
    router_tools: &[Tool],
    name: &str,
) -> Option<Tool> {
    if name != "axon"
        && let Some(tool) = router_tools.iter().find(|tool| tool.name == name)
    {
        return Some(tool.clone());
    }
    if name == "axon" {
        return projection_allows_legacy(projection)
            .then(|| {
                router_tools
                    .iter()
                    .find(|tool| tool.name == "axon")
                    .cloned()
            })
            .flatten();
    }
    if !projection_allows_atomic(projection) {
        return None;
    }
    let action = atomic_action_from_tool_name(name)?;
    let spec = server_authz::MCP_ACTION_SPECS
        .iter()
        .find(|spec| spec.name == action)?;
    Some(atomic_tool_for_spec(spec))
}

fn atomic_tool_for_spec(spec: &server_authz::McpActionSpec) -> Tool {
    let safety = spec.safety_hints();
    let annotations = ToolAnnotations::with_title(format!("Axon: {}", spec.name))
        .read_only(safety.read_only)
        .destructive(safety.destructive);
    let annotations = match safety.idempotent {
        Some(value) => annotations.idempotent(value),
        None => annotations,
    };

    Tool::new(
        atomic_tool_name(spec.name),
        spec.description,
        Arc::new(focused_input_schema(spec.name)),
    )
    .with_title(format!("Axon: {}", spec.name))
    .with_annotations(annotations)
}

fn action_matches(node: &Value, action: &str) -> bool {
    let Some(action_schema) = node.pointer("/properties/action") else {
        return false;
    };
    action_schema
        .get("const")
        .and_then(Value::as_str)
        .is_some_and(|value| value == action)
        || action_schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values.len() == 1 && values.first().and_then(Value::as_str) == Some(action)
            })
}

fn resolve_local_ref<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    reference
        .strip_prefix('#')
        .and_then(|pointer| root.pointer(pointer))
}

fn merge_schema_object(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, value) in source {
        match key.as_str() {
            "properties" => {
                let target_properties = target
                    .entry("properties".to_string())
                    .or_insert_with(|| json!({}));
                if let (Some(target_properties), Some(source_properties)) =
                    (target_properties.as_object_mut(), value.as_object())
                {
                    target_properties.extend(source_properties.clone());
                }
            }
            "required" => {
                let target_required = target
                    .entry("required".to_string())
                    .or_insert_with(|| json!([]));
                if let (Some(target_required), Some(source_required)) =
                    (target_required.as_array_mut(), value.as_array())
                {
                    for item in source_required {
                        if !target_required.contains(item) {
                            target_required.push(item.clone());
                        }
                    }
                }
            }
            "$defs" => {}
            _ => {
                target.entry(key).or_insert(value);
            }
        }
    }
}

fn materialize_schema(root: &Value, node: &Value) -> Map<String, Value> {
    let Some(object) = node.as_object() else {
        return Map::new();
    };

    let mut output = Map::new();
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some(resolved) = resolve_local_ref(root, reference)
    {
        merge_schema_object(&mut output, materialize_schema(root, resolved));
    }

    if let Some(all_of) = object.get("allOf").and_then(Value::as_array) {
        for item in all_of {
            merge_schema_object(&mut output, materialize_schema(root, item));
        }
    }

    for (key, value) in object {
        if key != "$ref" && key != "allOf" {
            merge_schema_object(&mut output, Map::from_iter([(key.clone(), value.clone())]));
        }
    }
    output
}

fn schema_from_discriminator_then(root: &Value, then_schema: &Value) -> Map<String, Value> {
    let mut output = Map::new();
    let Some(then_object) = then_schema.as_object() else {
        return output;
    };

    if let Some(body_schema) = then_object
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("body"))
    {
        merge_schema_object(&mut output, materialize_schema(root, body_schema));
    }

    if let Some(properties) = then_object.get("properties").and_then(Value::as_object) {
        let extra_properties = properties
            .iter()
            .filter(|(name, _)| name.as_str() != "body")
            .map(|(name, schema)| (name.clone(), schema.clone()))
            .collect::<Map<_, _>>();
        if !extra_properties.is_empty() {
            merge_schema_object(
                &mut output,
                Map::from_iter([("properties".to_string(), Value::Object(extra_properties))]),
            );
        }
    }

    if let Some(required) = then_object.get("required").cloned() {
        merge_schema_object(
            &mut output,
            Map::from_iter([("required".to_string(), required)]),
        );
    }
    output
}

fn find_action_schema(root: &Value, node: &Value, action: &str) -> Option<Map<String, Value>> {
    let object = node.as_object()?;

    if let Some(condition) = object.get("if")
        && action_matches(condition, action)
        && let Some(then_schema) = object.get("then")
    {
        return Some(schema_from_discriminator_then(root, then_schema));
    }

    if action_matches(node, action) {
        return Some(materialize_schema(root, node));
    }

    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some(resolved) = resolve_local_ref(root, reference)
        && let Some(found) = find_action_schema(root, resolved, action)
    {
        return Some(found);
    }

    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(items) = object.get(key).and_then(Value::as_array) {
            for item in items {
                if let Some(found) = find_action_schema(root, item, action) {
                    return Some(found);
                }
            }
        }
    }

    if let Some(defs) = object.get("$defs").and_then(Value::as_object) {
        for definition in defs.values() {
            if let Some(found) = find_action_schema(root, definition, action) {
                return Some(found);
            }
        }
    }

    None
}

pub(super) fn focused_input_schema(action: &str) -> rmcp::model::JsonObject {
    let aggregate = tool_schema::axon_tool_input_schema();
    let aggregate = Value::Object(aggregate.as_ref().clone());
    let mut output = find_action_schema(&aggregate, &aggregate, action).unwrap_or_default();

    output.insert("type".to_string(), json!("object"));
    output.insert("additionalProperties".to_string(), json!(false));

    if let Some(properties) = output.get_mut("properties").and_then(Value::as_object_mut) {
        properties.remove("action");
    }

    let runtime_required = aggregate
        .get("x-axon-required-fields")
        .and_then(Value::as_object)
        .and_then(|all| all.get(action))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let required = output
        .entry("required".to_string())
        .or_insert_with(|| json!([]));
    if let Some(required) = required.as_array_mut() {
        required.retain(|name| name.as_str() != Some("action"));
        for field in runtime_required {
            if !required.contains(&field) {
                required.push(field);
            }
        }
    }
    if output
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        output.remove("required");
    }

    if let Some(defs) = aggregate.get("$defs") {
        output.insert("$defs".to_string(), defs.clone());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_names_round_trip_for_every_live_action() {
        for spec in server_authz::MCP_ACTION_SPECS {
            let name = atomic_tool_name(spec.name);
            assert_eq!(atomic_action_from_tool_name(&name), Some(spec.name));
        }
        assert_eq!(atomic_action_from_tool_name("axon_not_real"), None);
        assert_eq!(atomic_action_from_tool_name("status"), None);
    }

    #[test]
    fn focused_schema_excludes_action_and_unrelated_fields() {
        let status = focused_input_schema("status");
        let properties = status["properties"].as_object().unwrap();
        assert!(!properties.contains_key("action"));
        assert!(!properties.contains_key("source"));

        let source = focused_input_schema("source");
        let properties = source["properties"].as_object().unwrap();
        assert!(properties.contains_key("source"));
        assert_eq!(source["required"], json!(["source"]));
        assert_eq!(source["additionalProperties"], json!(false));
    }

    #[test]
    fn atomic_projection_has_exactly_one_tool_per_live_action() {
        let tools = projected_tools(
            McpProjection::Atomic,
            vec![
                Tool::new("axon", "legacy", Arc::new(Map::new())),
                Tool::new("axon_status_dashboard", "dashboard", Arc::new(Map::new())),
            ],
        );
        assert_eq!(tools.len(), server_authz::MCP_ACTION_SPECS.len() + 1);
        for spec in server_authz::MCP_ACTION_SPECS {
            assert_eq!(
                tools
                    .iter()
                    .filter(|tool| tool.name == atomic_tool_name(spec.name))
                    .count(),
                1,
                "{}",
                spec.name
            );
        }
    }

    #[test]
    fn projection_modes_publish_expected_surfaces() {
        let router_tools = vec![
            Tool::new("axon", "legacy", Arc::new(Map::new())),
            Tool::new("axon_status_dashboard", "dashboard", Arc::new(Map::new())),
        ];

        let legacy_only = projected_tools(McpProjection::Legacy, router_tools.clone());
        assert_eq!(legacy_only.len(), 2);
        assert!(legacy_only.iter().any(|tool| tool.name == "axon"));
        assert!(
            legacy_only
                .iter()
                .any(|tool| tool.name == "axon_status_dashboard")
        );

        let atomic_only = projected_tools(McpProjection::Atomic, router_tools.clone());
        assert_eq!(atomic_only.len(), server_authz::MCP_ACTION_SPECS.len() + 1);
        assert!(atomic_only.iter().all(|tool| tool.name != "axon"));
        assert!(
            atomic_only
                .iter()
                .any(|tool| tool.name == "axon_status_dashboard")
        );

        let both = projected_tools(McpProjection::Both, router_tools);
        assert_eq!(both.len(), server_authz::MCP_ACTION_SPECS.len() + 2);
        assert_eq!(both.iter().filter(|tool| tool.name == "axon").count(), 1);
        assert!(both.iter().any(|tool| tool.name == "axon_status_dashboard"));
    }

    #[test]
    fn annotations_follow_registry_scope_without_overclaiming_write_idempotency() {
        let tools = projected_tools(
            McpProjection::Atomic,
            vec![Tool::new("axon", "legacy", Arc::new(Map::new()))],
        );

        for spec in server_authz::MCP_ACTION_SPECS {
            let tool = tools
                .iter()
                .find(|tool| tool.name == atomic_tool_name(spec.name))
                .unwrap();
            let annotations = tool.annotations.as_ref().unwrap();
            let safety = spec.safety_hints();
            assert_eq!(
                annotations.read_only_hint,
                Some(safety.read_only),
                "{}",
                spec.name
            );
            assert_eq!(
                annotations.destructive_hint,
                Some(safety.destructive),
                "{}",
                spec.name
            );
            assert_eq!(
                annotations.idempotent_hint, safety.idempotent,
                "{}",
                spec.name
            );
            assert_eq!(annotations.open_world_hint, None, "{}", spec.name);
        }
    }

    #[test]
    fn every_live_action_resolves_to_a_coherent_focused_schema() {
        for spec in server_authz::MCP_ACTION_SPECS {
            let focused = focused_input_schema(spec.name);
            let properties = focused["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{} has no focused properties object", spec.name));
            assert!(!properties.contains_key("action"), "{}", spec.name);
            assert_eq!(
                focused["additionalProperties"],
                json!(false),
                "{}",
                spec.name
            );

            if let Some(required) = focused.get("required").and_then(Value::as_array) {
                for field in required {
                    let field = field
                        .as_str()
                        .unwrap_or_else(|| panic!("{} has non-string required field", spec.name));
                    assert!(
                        properties.contains_key(field),
                        "{} requires missing property {field}",
                        spec.name
                    );
                }
            }
        }

        let codex = focused_input_schema("codex");
        assert!(
            codex["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|field| field == "subaction"))
        );
        assert!(codex["properties"].get("subaction").is_some());
        assert!(codex["properties"].get("source").is_none());
    }

    #[test]
    fn auxiliary_tools_remain_addressable_in_every_projection() {
        let router_tools = vec![
            Tool::new("axon", "legacy", Arc::new(Map::new())),
            Tool::new("axon_status_dashboard", "dashboard", Arc::new(Map::new())),
        ];

        for projection in [
            McpProjection::Legacy,
            McpProjection::Atomic,
            McpProjection::Both,
        ] {
            assert!(projected_tool(projection, &router_tools, "axon_status_dashboard").is_some());
        }
    }
}
