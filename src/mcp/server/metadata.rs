use rmcp::model::{ExtensionCapabilities, Meta, ServerCapabilities};
use std::sync::LazyLock;

pub const STATUS_DASHBOARD_URI: &str = "ui://axon/status-dashboard";
pub const MCP_APP_MIME_TYPE: &str = "text/html;profile=mcp-app";

pub static STATUS_DASHBOARD_HTML: &str = include_str!("../assets/status_dashboard.html");

pub static MCP_TOOL_SCHEMA_MD: LazyLock<String> = LazyLock::new(|| {
    use super::common::MCP_TOOL_SCHEMA_URI;
    use crate::mcp::schema::AxonRequest;

    let schema = rmcp::schemars::schema_for!(AxonRequest);
    let schema_json = serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string());
    format!(
        "# Axon MCP Tool Schema\n\nURI: `{}`\n\nSingle tool name: `axon`\n\nRouting contract:\n- `action` is required\n- `subaction` is required for subaction families\n- `response_mode` supports `path|inline|both|auto_inline`; most actions default to `path`, while `scrape` and `retrieve` default to inline paged document reads\n\n## JSON Schema\n\n```json\n{}\n```\n",
        MCP_TOOL_SCHEMA_URI, schema_json
    )
});

pub fn mcp_tool_schema_markdown() -> &'static str {
    &MCP_TOOL_SCHEMA_MD
}

pub fn axon_tool_meta() -> Meta {
    let mut m = Meta::new();
    // Nested form: _meta.ui.resourceUri (TypeScript SDK / MCP Apps convention).
    // The MCP host SDK normalizes this into a flat key internally; we only need
    // to emit the canonical nested form here.
    m.insert(
        "ui".to_string(),
        serde_json::json!({ "resourceUri": STATUS_DASHBOARD_URI }),
    );
    m
}

pub fn mcp_apps_server_capabilities() -> ServerCapabilities {
    let mut extensions = ExtensionCapabilities::new();
    // Declare MCP Apps extension support so the host knows to render widgets.
    let mut ui_ext = serde_json::Map::new();
    ui_ext.insert(
        "mimeTypes".to_string(),
        serde_json::json!(["text/html;profile=mcp-app"]),
    );
    extensions.insert("io.modelcontextprotocol/ui".to_string(), ui_ext);
    ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .enable_extensions_with(extensions)
        .build()
}
