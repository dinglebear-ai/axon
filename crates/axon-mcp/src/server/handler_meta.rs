use super::AxonMcpServer;
use super::common::MCP_TOOL_SCHEMA_URI;
use crate::schema::AxonToolResponse;
use axon_services::system;
use rmcp::{
    ErrorData, RoleServer,
    model::{
        ExtensionCapabilities, InitializeRequestParams, InitializeResult, ListResourcesResult,
        MetaObject, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
};
use std::sync::LazyLock;

pub(crate) const STATUS_DASHBOARD_URI: &str = "ui://axon/status-dashboard";
pub(crate) const MCP_APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
static STATUS_DASHBOARD_HTML: &str = include_str!("../assets/status_dashboard.html");
static MCP_TOOL_SCHEMA_MD: LazyLock<String> =
    LazyLock::new(super::tool_schema::mcp_tool_schema_markdown);

pub(super) fn append_stale_binary_warning(response: AxonToolResponse) -> AxonToolResponse {
    match axon_core::binary_status::stale_binary_warning() {
        Some(warning) => response.with_warning(warning),
        None => response,
    }
}

pub(super) fn status_dashboard_tool_meta() -> MetaObject {
    let mut m = MetaObject::new();
    // Nested form: _meta.ui.resourceUri (TypeScript SDK / MCP Apps convention).
    // The MCP host SDK normalizes this into a flat key internally; we only need
    // to emit the canonical nested form here.
    m.insert(
        "ui".to_string(),
        serde_json::json!({ "resourceUri": STATUS_DASHBOARD_URI }),
    );
    m
}

pub(crate) fn status_dashboard_resource_meta() -> MetaObject {
    let mut m = MetaObject::new();
    m.insert(
        "ui".to_string(),
        serde_json::json!({
            "csp": {
                "connectDomains": [],
                "resourceDomains": [],
                "frameDomains": [],
                "baseUriDomains": []
            },
            "permissions": {}
        }),
    );
    m
}

pub(crate) fn mcp_apps_server_capabilities() -> ServerCapabilities {
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
        // SEP-2663 declares tasks through the SAME `extensions` map (the old
        // standalone `tasks` capability and `enable_tasks_with` are gone), and
        // `enable_extensions_with` REPLACES that map wholesale. This call must
        // therefore stay after it — inverting the order silently drops the
        // tasks capability and every task-augmented `tools/call` is rejected.
        .enable_tasks()
        .build()
}

pub(super) async fn initialize(
    server: &AxonMcpServer,
    request: InitializeRequestParams,
) -> Result<InitializeResult, ErrorData> {
    tracing::info!(
        client_name = %request.client_info.name,
        client_version = %request.client_info.version,
        protocol_version = %request.protocol_version,
        has_extensions = %request.capabilities.extensions.is_some(),
        extensions = ?request.capabilities.extensions,
        "mcp_app initialize — client capabilities"
    );
    let info = get_info(server);
    Ok(InitializeResult::new(info.capabilities)
        .with_protocol_version(request.protocol_version)
        .with_server_info(info.server_info)
        .with_instructions(info.instructions.unwrap_or_default()))
}

pub(super) fn get_info(_server: &AxonMcpServer) -> ServerInfo {
    tracing::info!("mcp_app get_info called — client connected");
    let mut info = ServerInfo::default();
    info.instructions = Some(concat!(
        "Axon is a self-hosted RAG engine for source indexing, extraction, and semantic search.\n",
        "\n",
        "Use the single `axon` tool with `action`/`subaction` routing for all operations.\n",
        "Call `action:help` first to discover all available actions, subactions, and parameter defaults.\n",
        "\n",
        "Search for this server's tools when the user wants to:\n",
        "- Index any source — a local path, git/web/feed/youtube/reddit/session/registry target — with `action:source`\n",
        "- Run semantic search or RAG queries over indexed content\n",
        "- Ask grounded questions against indexed docs (RAG with LLM synthesis)\n",
        "- Summarize one or more URLs from freshly scraped page context\n",
        "- Research topics via web search with automatic indexing\n",
        "- Extract structured data from pages using LLM-powered extraction\n",
        "- Check job queue status, cancel jobs, or manage async workers\n",
        "- Take screenshots, map site URLs, retrieve stored documents\n",
        "\n",
        "Key capabilities:\n",
        "- `source` — acquire and index one source (local path, git/web/feed/youtube/reddit/session/registry) through the unified pipeline\n",
        "- `query` — dense + BM42 hybrid semantic search\n",
        "- `endpoints` — static endpoint discovery with optional verification\n",
        "- `ask` — RAG: retrieve context + LLM answer\n",
        "- `summarize` — scrape URL context + configured LLM summary\n",
        "- `evaluate` — compare RAG quality against a baseline with judge diagnostics\n",
        "- `suggest` — propose new source targets from indexed coverage\n",
        "- `research` — SearXNG/Tavily web research with LLM synthesis and auto-indexing\n",
        "- `extract` — structured data extraction via LLM\n",
        "- `status` / `doctor` — job queue health and service diagnostics\n",
        "- MCP Apps enabled — exposes `ui://axon/status-dashboard` for live queue status widgets\n",
        "\n",
        "The `extract` async operation returns a job_id. Poll with `action=jobs`, `subaction=get`, and the returned `job_id`.\n",
        "Task-augmented calls are also supported for `extract.start`; use `tasks/get`, `tasks/cancel`, and `_meta.progressToken` for protocol-level task flows. Per SEP-2663 the terminal result is returned inline by `tasks/get` — there is no separate `tasks/result` or `tasks/list`."
    ).into());
    info.capabilities = mcp_apps_server_capabilities();
    info
}

pub(super) async fn list_resources(
    _server: &AxonMcpServer,
    _request: Option<PaginatedRequestParams>,
    _context: RequestContext<RoleServer>,
) -> Result<ListResourcesResult, ErrorData> {
    tracing::info!("mcp_app list_resources called");
    // `RawResource` + `AnnotateAble::no_annotation()` are gone in rmcp 3.x;
    // `Resource` is now built directly with its own builder methods.
    let schema_resource = Resource::new(MCP_TOOL_SCHEMA_URI, "mcp-tool-schema")
        .with_title("Axon MCP Tool Schema")
        .with_description("Source-of-truth schema and routing contract for the unified axon tool")
        .with_mime_type("text/markdown");

    let dashboard_resource = Resource::new(STATUS_DASHBOARD_URI, "status-dashboard")
        .with_title("Axon Status Dashboard")
        .with_description(
            "Interactive MCP App widget showing live job queue status for all Axon workers",
        )
        .with_mime_type(MCP_APP_MIME_TYPE)
        .with_meta(status_dashboard_resource_meta());

    Ok(ListResourcesResult::with_all_items(vec![
        schema_resource,
        dashboard_resource,
    ]))
}

pub(super) async fn read_resource(
    server: &AxonMcpServer,
    request: ReadResourceRequestParams,
    _context: RequestContext<RoleServer>,
) -> Result<ReadResourceResult, ErrorData> {
    tracing::info!(uri = %request.uri, "mcp_app read_resource called");
    if request.uri == STATUS_DASHBOARD_URI {
        // Inject current status data so the widget renders immediately, bypassing
        // the MCP Apps postMessage bridge which may not be available in all hosts.
        let status_json = match server.base_service_context().await {
            Ok(ctx) => match system::full_status(&ctx).await {
                Ok(r) => serde_json::to_string(&r.payload).unwrap_or_else(|_| "null".to_string()),
                Err(_) => "null".to_string(),
            },
            Err(_) => "null".to_string(),
        };
        let html = STATUS_DASHBOARD_HTML.replacen(
            "window.__AXON_INITIAL_STATUS__ = null;",
            &format!("window.__AXON_INITIAL_STATUS__ = {};", status_json),
            1,
        );
        return Ok(ReadResourceResult::new(vec![
            ResourceContents::TextResourceContents {
                uri: STATUS_DASHBOARD_URI.to_string(),
                mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
                text: html,
                meta: Some(status_dashboard_resource_meta()),
            },
        ]));
    }
    if request.uri != MCP_TOOL_SCHEMA_URI {
        return Err(ErrorData::invalid_params(
            format!("resource not found: {}", request.uri),
            None,
        ));
    }
    Ok(ReadResourceResult::new(vec![
        ResourceContents::TextResourceContents {
            uri: MCP_TOOL_SCHEMA_URI.to_string(),
            mime_type: Some("text/markdown".to_string()),
            text: MCP_TOOL_SCHEMA_MD.to_string(),
            meta: None,
        },
    ]))
}
