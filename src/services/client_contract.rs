use crate::core::config::{RenderMode, ScrapeFormat};
use serde::{Deserialize, Serialize};

#[path = "client_contract/contracts.rs"]
mod contracts;
pub use contracts::{RestRouteContract, rest_route_contracts};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestExtractMode {
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestIngestSourceType {
    Github,
    Gitlab,
    Gitea,
    Git,
    Reddit,
    Youtube,
    Sessions,
}

impl From<RestExtractMode> for ClientExtractMode {
    fn from(value: RestExtractMode) -> Self {
        match value {
            RestExtractMode::Auto => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestCrawlRequest {
    pub urls: Vec<String>,
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub render_mode: Option<RenderMode>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub max_sitemaps: Option<usize>,
    pub sitemap_since_days: Option<u32>,
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
    pub delay_ms: Option<u64>,
    pub collection: Option<String>,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestScrapeRequest {
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub render_mode: Option<RenderMode>,
    pub format: Option<ScrapeFormat>,
    pub embed: Option<bool>,
    pub collection: Option<String>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestExtractRequest {
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    #[serde(alias = "extract_mode")]
    pub mode: Option<RestExtractMode>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<RenderMode>,
    pub embed: Option<bool>,
    pub collection: Option<String>,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestEmbedRequest {
    pub input: String,
    pub source_type: Option<String>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestQueryRequest {
    pub query: String,
    pub collection: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestRetrieveRequest {
    pub url: String,
    pub collection: Option<String>,
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestEvaluateRequest {
    pub question: String,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestSuggestRequest {
    pub focus: Option<String>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestMapRequest {
    pub url: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestSearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub time_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestResearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub time_range: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestAskRequest {
    pub query: String,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub diagnostics: Option<bool>,
    #[serde(default)]
    pub explain: Option<bool>,
    #[serde(default)]
    pub hybrid_search: Option<bool>,
    #[serde(default)]
    pub ask_chunk_limit: Option<usize>,
    #[serde(default)]
    pub ask_full_docs: Option<usize>,
    #[serde(default)]
    pub ask_max_context_chars: Option<usize>,
    #[serde(default)]
    pub ask_hybrid_candidates: Option<usize>,
    #[serde(default)]
    pub ask_min_relevance_score: Option<f64>,
    #[serde(default)]
    pub ask_doc_chunk_limit: Option<usize>,
    #[serde(default)]
    pub ask_doc_fetch_concurrency: Option<usize>,
    #[serde(default)]
    pub ask_backfill_chunks: Option<usize>,
    #[serde(default)]
    pub ask_candidate_limit: Option<usize>,
    #[serde(default)]
    pub ask_min_citations_nontrivial: Option<usize>,
    #[serde(default)]
    pub ask_authoritative_domains: Option<Vec<String>>,
    #[serde(default)]
    pub ask_authoritative_boost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestChatRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestChatResponse {
    pub message: String,
    pub answer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestSummarizeRequest {
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub render_mode: Option<RenderMode>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestBrandRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestDiffRequest {
    pub url_a: String,
    pub url_b: String,
    pub render_mode: Option<RenderMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestScreenshotRequest {
    pub url: String,
    pub viewport: Option<String>,
    pub full_page: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestIngestRequest {
    pub source_type: RestIngestSourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions: Option<RestSessionsIngestOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestSessionsIngestOptions {
    pub claude: Option<bool>,
    pub codex: Option<bool>,
    pub gemini: Option<bool>,
    pub project: Option<String>,
}

impl From<RestIngestSourceType> for crate::mcp::schema::IngestSourceType {
    fn from(value: RestIngestSourceType) -> Self {
        match value {
            RestIngestSourceType::Github => Self::Github,
            RestIngestSourceType::Gitlab => Self::Gitlab,
            RestIngestSourceType::Gitea => Self::Gitea,
            RestIngestSourceType::Git => Self::Git,
            RestIngestSourceType::Reddit => Self::Reddit,
            RestIngestSourceType::Youtube => Self::Youtube,
            RestIngestSourceType::Sessions => Self::Sessions,
        }
    }
}

impl From<RestIngestRequest> for crate::mcp::schema::IngestRequest {
    fn from(req: RestIngestRequest) -> Self {
        Self {
            source_type: Some(req.source_type.into()),
            target: req.target,
            include_source: req.include_source,
            sessions: req.sessions.map(Into::into),
            ..Default::default()
        }
    }
}

impl From<RestSessionsIngestOptions> for crate::mcp::schema::SessionsIngestOptions {
    fn from(value: RestSessionsIngestOptions) -> Self {
        Self {
            claude: value.claude,
            codex: value.codex,
            gemini: value.gemini,
            project: value.project,
        }
    }
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
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub route_preference: ClientRoutePreference,
}

impl From<ClientCrawlRequest> for RestCrawlRequest {
    fn from(req: ClientCrawlRequest) -> Self {
        Self {
            urls: req.urls,
            max_pages: req.max_pages,
            max_depth: req.max_depth,
            render_mode: req.render_mode,
            include_subdomains: req.include_subdomains,
            respect_robots: req.respect_robots,
            discover_sitemaps: req.discover_sitemaps,
            max_sitemaps: req.max_sitemaps,
            sitemap_since_days: req.sitemap_since_days,
            discover_llms_txt: req.discover_llms_txt,
            max_llms_txt_urls: req.max_llms_txt_urls,
            delay_ms: req.delay_ms,
            collection: None,
            headers: req
                .headers
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect(),
        }
    }
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

impl From<ClientScrapeRequest> for RestScrapeRequest {
    fn from(req: ClientScrapeRequest) -> Self {
        Self {
            url: Some(req.url),
            urls: None,
            render_mode: req.render_mode,
            format: req.format,
            embed: req.embed,
            collection: None,
            root_selector: req.root_selector,
            exclude_selector: req.exclude_selector,
            headers: req
                .headers
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect(),
        }
    }
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

impl From<ClientExtractRequest> for RestExtractRequest {
    fn from(req: ClientExtractRequest) -> Self {
        let mode = match req.mode {
            Some(ClientExtractMode::Auto) => Some(RestExtractMode::Auto),
            _ => None,
        };
        Self {
            urls: req.urls,
            prompt: req.prompt,
            mode,
            max_pages: req.max_pages,
            render_mode: req.render_mode,
            embed: req.embed,
            collection: None,
            headers: req
                .headers
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect(),
        }
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

impl From<ClientEmbedRequest> for RestEmbedRequest {
    fn from(req: ClientEmbedRequest) -> Self {
        Self {
            input: req.input,
            source_type: req.source_type,
            collection: req.collection,
        }
    }
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

impl From<ClientQueryRequest> for RestQueryRequest {
    fn from(req: ClientQueryRequest) -> Self {
        Self {
            query: req.query,
            collection: req.collection,
            limit: req.limit,
            offset: req.offset,
        }
    }
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

impl From<ClientRetrieveRequest> for RestRetrieveRequest {
    fn from(req: ClientRetrieveRequest) -> Self {
        Self {
            url: req.url,
            collection: None,
            max_points: req.max_points,
            cursor: req.cursor,
            token_budget: req.token_budget,
        }
    }
}
