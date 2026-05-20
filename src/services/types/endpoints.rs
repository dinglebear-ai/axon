#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointOptions {
    pub include_bundles: bool,
    pub first_party_only: bool,
    pub unique_only: bool,
    pub max_scripts: usize,
    pub max_scan_bytes: usize,
    pub verify: bool,
    pub capture_network: bool,
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
        }
    }
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSourceKind {
    InlineScript,
    ScriptBundle,
    HtmlAttribute,
    NetworkCapture,
}

impl EndpointSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InlineScript => "inline_script",
            Self::ScriptBundle => "script_bundle",
            Self::HtmlAttribute => "html_attribute",
            Self::NetworkCapture => "network_capture",
        }
    }
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
}
