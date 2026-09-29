//! The public MCP views own request shapes and finite selectors.
use serde_json::Value;

pub(crate) fn request_schema_for(request_dto: &str) -> Value {
    axon_mcp::schema_registry::request_schema_for(request_dto)
}

pub(crate) fn typed_subaction_variants(action: &str) -> Vec<String> {
    axon_mcp::schema_registry::subaction_variants(action)
}
