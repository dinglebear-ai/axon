#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointOptions {
    pub include_bundles: bool,
    pub first_party_only: bool,
    pub unique_only: bool,
    pub max_scripts: usize,
    pub max_scan_bytes: usize,
    pub verify: bool,
    pub capture_network: bool,
    pub probe_rpc: bool,
    /// Additionally synthesize + probe `mcp.<registrable-apex>` candidates.
    /// No-op unless `probe_rpc` is also set.
    pub probe_rpc_subdomains: bool,
}

impl Default for EndpointOptions {
    fn default() -> Self {
        Self {
            include_bundles: true,
            first_party_only: false,
            unique_only: true,
            max_scripts: 40,
            max_scan_bytes: 8 * 1024 * 1024,
            verify: false,
            capture_network: false,
            probe_rpc: false,
            probe_rpc_subdomains: false,
        }
    }
}

/// Detected RPC protocol family. Wire strings match the historical
/// stringly-typed values (`"jsonrpc2"`, `"openrpc"`, `"mcp"`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum RpcProtocol {
    /// Generic JSON-RPC 2.0 (detected via `system.listMethods` or a `-32601` error).
    // Explicit rename pins the backward-compatible wire string independently of
    // `rename_all` (which would also yield `"jsonrpc2"` today, but is not a contract).
    #[serde(rename = "jsonrpc2")]
    Jsonrpc2,
    /// OpenRPC service (responded to `rpc.discover` with an `openrpc` document).
    Openrpc,
    /// Model Context Protocol server — detected via a successful `initialize`
    /// handshake, or inferred from an SSE (`text/event-stream`) transport.
    Mcp,
}

impl RpcProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jsonrpc2 => "jsonrpc2",
            Self::Openrpc => "openrpc",
            Self::Mcp => "mcp",
        }
    }
}

/// Transport layer the protocol was observed over.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum RpcTransport {
    /// Request/response over HTTP POST (JSON or streamed SSE response body).
    Http,
    /// Long-lived Server-Sent Events transport (`text/event-stream` on GET).
    Sse,
}

impl RpcTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

/// Result of probing a discovered endpoint for JSON-RPC 2.0 / OpenRPC / MCP support.
///
/// Fields are populated according to the detected `protocol` and are otherwise
/// left empty: `server_name`/`server_version`/`tools` are MCP-only, `methods` is
/// JSON-RPC/OpenRPC-only. The flat shape preserves the wire contract; the type
/// does not statically prevent contradictory combinations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RpcProbeResult {
    /// Detected protocol, or `null` when no protocol matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<RpcProtocol>,
    /// Transport the protocol was observed over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<RpcTransport>,
    /// MCP `serverInfo.name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// MCP `serverInfo.version`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    /// Discovered method names (`system.listMethods` or OpenRPC `methods[].name`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    /// MCP tool names from `tools/list`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    RelativePath,
    AbsoluteUrl,
    Graphql,
    Websocket,
}

impl EndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelativePath => "relative_path",
            Self::AbsoluteUrl => "absolute_url",
            Self::Graphql => "graphql",
            Self::Websocket => "websocket",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSourceKind {
    InlineScript,
    ScriptBundle,
    HtmlAttribute,
    NetworkCapture,
    /// Not discovered in the page — synthesized from the target URL and
    /// confirmed by an RPC probe (well-known MCP path or `mcp.<apex>` host).
    SynthesizedMcp,
}

impl EndpointSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InlineScript => "inline_script",
            Self::ScriptBundle => "script_bundle",
            Self::HtmlAttribute => "html_attribute",
            Self::NetworkCapture => "network_capture",
            Self::SynthesizedMcp => "synthesized_mcp",
        }
    }
}

/// Which host a synthesized MCP candidate targets.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum McpHostKind {
    /// Same host as the target URL (e.g. `foo.com/mcp`).
    SameHost,
    /// `mcp.<registrable-apex>` subdomain (e.g. `mcp.foo.com/mcp`).
    ApexSubdomain,
}

/// Outcome of probing one synthesized MCP candidate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum McpProbeOutcome {
    /// Returned a positive JSON-RPC/MCP signal.
    Confirmed,
    /// Reachable validation passed but no positive RPC signal (404, non-RPC
    /// JSON, transport error, or timeout all fold here).
    Unconfirmed,
    /// Rejected by the SSRF guard before any request was made.
    Blocked,
}

/// One synthesized MCP candidate and its probe outcome.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct McpCandidateAttempt {
    pub url: String,
    pub host_kind: McpHostKind,
    pub path: String,
    pub outcome: McpProbeOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_probe: Option<RpcProbeResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct DiscoveredEndpoint {
    pub value: String,
    pub normalized_url: Option<String>,
    pub kind: EndpointKind,
    pub first_party: bool,
    pub source: EndpointSourceKind,
    pub source_url: Option<String>,
    pub verified: Option<EndpointVerification>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_probe: Option<RpcProbeResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct EndpointVerification {
    pub attempted_url: String,
    pub method: String,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub final_url: Option<String>,
    pub redirect_count: usize,
    pub reachable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct EndpointReport {
    pub url: String,
    pub endpoints: Vec<DiscoveredEndpoint>,
    pub hosts: Vec<String>,
    pub scripts_discovered: usize,
    pub bundles_fetched: usize,
    pub bundles_scanned: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
    /// Synthesized MCP candidate probe attempts (omitted when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_candidates: Vec<McpCandidateAttempt>,
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
