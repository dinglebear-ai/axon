use crate::core::config::{RenderMode, ScrapeFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientRoutePreference {
    #[default]
    Default,
    LocalOnly,
    ServerRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientExtractMode {
    Auto,
    Deterministic,
    Llm,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientCrawlRequest {
    pub urls: Vec<String>,
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub render_mode: Option<RenderMode>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub max_sitemaps: Option<usize>,
    pub sitemap_since_days: Option<u32>,
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientScrapeRequest {
    pub url: String,
    pub render_mode: Option<RenderMode>,
    pub format: Option<ScrapeFormat>,
    pub embed: Option<bool>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientExtractRequest {
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    #[serde(alias = "extract_mode")]
    pub mode: Option<ClientExtractMode>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<RenderMode>,
    pub embed: Option<bool>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

impl ClientExtractRequest {
    pub fn effective_mode(&self) -> ClientExtractMode {
        self.mode.unwrap_or(ClientExtractMode::Auto)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientEmbedRequest {
    pub input: String,
    pub source_type: Option<String>,
    pub collection: Option<String>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientQueryRequest {
    pub query: String,
    pub collection: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientRetrieveRequest {
    pub url: String,
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}
