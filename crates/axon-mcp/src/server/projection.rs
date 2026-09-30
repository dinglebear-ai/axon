//! Startup-static composition of aggregate, canonical leaf and auxiliary routes.
use super::{AxonMcpServer, operations};
use axon_core::config::McpToolProjection;
use rmcp::{ErrorData, handler::server::tool::ToolRouter, model::CallToolRequestParams};
use serde_json::Value;

pub(super) fn build_router(projection: McpToolProjection) -> ToolRouter<AxonMcpServer> {
    compose_router(projection, AxonMcpServer::tool_router())
}

pub(super) fn compose_router(
    projection: McpToolProjection,
    mut router: ToolRouter<AxonMcpServer>,
) -> ToolRouter<AxonMcpServer> {
    let dispatch = router
        .map
        .get("axon")
        .expect("canonical dispatch route")
        .clone();
    if projection == McpToolProjection::Atomic {
        router.remove_route("axon");
    }
    if projection != McpToolProjection::Legacy {
        for operation in operations::operation_registry() {
            assert!(
                operation.name != "axon" && !router.has_route(&operation.name),
                "MCP operation collides with another route: {}",
                operation.name
            );
            // Reuse the original handler closure, never a second dispatcher or
            // an internal MCP round-trip.
            let mut route = dispatch.clone();
            route.attr = operation.tool();
            router.add_route(route);
        }
    }
    router
}

/// Resolve before policy/task admission, preserving the presented name for
/// audit. The result distinguishes canonical calls from auxiliary tools even
/// when an auxiliary caller supplies forged action arguments.
pub(super) fn normalize_projected_tool_call(
    server: &AxonMcpServer,
    request: &mut CallToolRequestParams,
) -> Result<bool, ErrorData> {
    if !server.projected_router.has_route(request.name.as_ref()) {
        return Err(ErrorData::invalid_params(
            format!(
                "tool {} is unavailable in {} projection",
                request.name, server.cfg.mcp_tool_projection
            ),
            None,
        ));
    }
    if request.name.as_ref() == "axon" {
        return Ok(true);
    }
    let Some(operation) = operations::operation_named(request.name.as_ref()) else {
        return Ok(false);
    };
    let arguments = request.arguments.get_or_insert_with(Default::default);
    for field in ["action", "subaction"] {
        if arguments.contains_key(field) {
            return Err(ErrorData::invalid_params(
                format!(
                    "{field} is fixed by tool {} and must not be supplied",
                    request.name
                ),
                None,
            ));
        }
    }
    let properties = operation
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("atomic operation properties");
    for field in arguments.keys() {
        if !properties.contains_key(field) {
            return Err(ErrorData::invalid_params(
                format!("{field} is not an input to tool {}", request.name),
                None,
            ));
        }
    }
    arguments.insert("action".into(), Value::String(operation.action.to_owned()));
    if let Some(subaction) = &operation.subaction {
        arguments.insert("subaction".into(), Value::String(subaction.clone()));
    }
    Ok(true)
}
