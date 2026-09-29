//! Public schema-generation views of the live MCP registry. No mirrored names.
use crate::server::{operation_registry, operation_schema, server_authz, tool_schema};
use serde_json::Value;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpActionSpec {
    pub action: &'static str,
    pub description: &'static str,
    pub request_dto: &'static str,
    pub result_dto: &'static str,
    pub required_scope: &'static str,
    pub mutates: bool,
    pub async_job: bool,
}

pub fn action_registry() -> &'static [McpActionSpec] {
    static ACTIONS: LazyLock<Vec<McpActionSpec>> = LazyLock::new(|| {
        server_authz::MCP_ACTION_SPECS
            .iter()
            .map(|spec| McpActionSpec {
                action: spec.name,
                description: spec.description,
                request_dto: spec.request_dto,
                result_dto: "AxonToolResponse",
                required_scope: spec.scope.as_label(),
                mutates: operation_registry()
                    .iter()
                    .any(|op| op.action == spec.name && !op.read_only),
                async_job: spec.async_job,
            })
            .collect()
    });
    &ACTIONS
}

/// The same canonical action schema used to derive leaf tools, including local
/// route selectors and transport-specific DTOs. All referenced definitions are
/// closed; the fixed action is omitted, while subaction remains caller-owned.
pub fn request_schema_for(request_dto: &str) -> Value {
    let mut matches = action_registry()
        .iter()
        .filter(|spec| spec.request_dto == request_dto);
    let spec = matches
        .next()
        .unwrap_or_else(|| panic!("no canonical MCP request DTO {request_dto}"));
    assert!(
        matches.next().is_none(),
        "duplicate MCP request DTO {request_dto}"
    );
    Value::Object(operation_schema::focused(
        &tool_schema::canonical_request_schema(),
        spec.action,
        None,
    ))
}

pub fn subaction_variants(action: &str) -> Vec<String> {
    let root = tool_schema::canonical_request_schema();
    operation_schema::subactions(&root, operation_schema::action_branch(&root, action))
}

pub fn canonical_request_schema() -> Value {
    tool_schema::canonical_request_schema()
}

pub fn operation_catalog() -> Vec<Value> {
    operation_registry()
        .iter()
        .map(|operation| {
            let mut entry = operation.metadata();
            entry["inputSchema"] = Value::Object(operation.input_schema.as_ref().clone());
            entry
        })
        .collect()
}

#[cfg(test)]
#[path = "schema_registry_tests.rs"]
mod tests;
