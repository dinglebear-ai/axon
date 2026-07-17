use axon_core::config::RenderMode;
use serde::{Deserialize, Serialize};

#[path = "client_contract/contracts.rs"]
mod contracts;
pub use contracts::{RestRouteContract, rest_route_contracts};
#[path = "client_contract/memory.rs"]
mod memory;
pub use memory::{RestMemoryEdgeType, RestMemoryNodeType, RestMemoryRequest, RestMemorySubaction};
#[path = "client_contract/sessions.rs"]
mod sessions;
pub use sessions::RestSessionsIngestOptions;

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

impl From<RestExtractMode> for ClientExtractMode {
    fn from(value: RestExtractMode) -> Self {
        match value {
            RestExtractMode::Auto => Self::Auto,
        }
    }
}

// ── Remaining forbidden-fork audit (WS-E / Q2-06) ───────────────────────────
//
// Every `Rest*Request` below is a genuine fork of an `axon_api::mcp_schema`
// counterpart (`ExtractRequest`, `QueryRequest`, `RetrieveRequest`,
// `EvaluateRequest`, `SuggestRequest`, `MapRequest`, `SearchRequest`,
// `ResearchRequest`, `AskRequest`, `SummarizeRequest`, `BrandRequest`,
// `DiffRequest`, `ScreenshotRequest`) or, for `RestChatRequest`/
// `RestChatResponse`, has no MCP/axon-api counterpart at all. None of them
// were converted to `pub use axon_api::... as Rest*;` re-export aliases
// because every one has a field-level shape difference from its MCP
// counterpart — aliasing would silently change the REST wire contract (this
// struct's `#[serde(deny_unknown_fields)]` currently rejects fields the MCP
// DTO accepts, or vice versa). Per the Forbidden DTO Forks rule ("where the
// canonical DTO differs in shape ... leave that fork in place and list it as
// a followup with the exact field-level diff"), the diffs are:
//
// - `RestExtractRequest` vs `mcp_schema::ExtractRequest`: MCP's is a
//   job-management action DTO (`subaction`, `job_id`, `limit`, `offset`,
//   `response_mode`) for the async extract job lifecycle; REST's is a
//   one-shot submission body (`collection`, `headers: Vec<String>`). No
//   fields in common beyond `urls`/`prompt`/`max_pages`/`render_mode`/`embed`;
//   not adaptable without changing one side's semantics.
// - `RestQueryRequest` vs `QueryRequest`: identical field set
//   (`query`/`collection`/`limit`/`offset`/`since`/`before`/`hybrid_search`)
//   plus MCP-only `response_mode` (controls path/inline delivery, meaningless
//   over REST, which is always inline).
// - `RestRetrieveRequest` vs `RetrieveRequest`: identical fields
//   (`url`/`collection`/`since`/`before`/`max_points`/`cursor`/
//   `token_budget`) plus MCP-only `response_mode`.
// - `RestEvaluateRequest` vs `EvaluateRequest`: REST's required field is
//   named `question`; MCP's is `query` with `#[serde(alias = "question")]`.
//   Otherwise identical (`collection`/`diagnostics`/`retrieval_ab`/`since`/
//   `before`/`hybrid_search`) plus MCP-only `response_mode`.
// - `RestSuggestRequest` vs `SuggestRequest`: MCP adds `limit` (REST has no
//   result-count cap) plus `response_mode`; both alias `focus`/`query`
//   naming loosely (MCP: `focus` aliased from `query`; REST: bare `focus`).
// - `RestMapRequest` vs `MapRequest`: identical (`url`/`limit`/`offset`)
//   plus MCP-only `response_mode`.
// - `RestSearchRequest` vs `SearchRequest`: REST's `time_range: Option<String>`
//   (free-form: "day"/"week"/etc. parsed downstream) vs MCP's
//   `search_time_range: Option<SearchTimeRange>` (closed enum) — different
//   field name AND type, plus MCP-only `response_mode`.
// - `RestResearchRequest` vs `ResearchRequest`: same `time_range` (String) vs
//   `search_time_range` (enum) name/type diff as Search, plus `response_mode`.
// - `RestAskRequest` vs `AskRequest`: field-for-field identical across all
//   `ask_*` tuning knobs plus `query`/`collection`/`since`/`before`/
//   `diagnostics`/`explain`/`hybrid_search`; only diff is MCP-only
//   `response_mode`. Closest candidate for a future alias if `response_mode`
//   is ever made universally ignorable by REST deserialization.
// - `RestSummarizeRequest` vs `SummarizeRequest`: REST adds `headers:
//   Vec<String>` (custom fetch headers, REST-only capability); MCP adds
//   `response_mode`. Otherwise identical (`url`/`urls`/`render_mode`/
//   `root_selector`/`exclude_selector`).
// - `RestBrandRequest` vs `BrandRequest`: REST has only `url`; MCP adds
//   `render_mode` (currently unused by the handler) and `response_mode`.
// - `RestDiffRequest` vs `DiffRequest`: identical (`url_a`/`url_b`/
//   `render_mode`) plus MCP-only `response_mode`.
// - `RestScreenshotRequest` vs `ScreenshotRequest`: MCP adds `output` (path
//   override for saved screenshot) and `response_mode`; otherwise identical
//   (`url`/`viewport`/`full_page`).
// - `RestChatRequest`/`RestChatResponse`: no MCP or axon-api counterpart
//   exists (`chat` is a REST/web-panel-only demo endpoint). Not a fork of an
//   existing canonical DTO; flagged here only because it currently has no
//   axon-api home. Candidate followup: promote to a canonical `axon-api`
//   DTO if `chat` gains MCP/CLI parity, otherwise leave as REST-only.
//
// Rule (1) dead-route compat DTOs (`RestCrawlRequest`, `RestScrapeRequest`,
// `RestEmbedRequest`) were
// deleted in a prior pass of this cleanup; they never appeared in
// `docs/reference/api/schemas.json` (regen was not required for that step).
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
pub struct RestQueryRequest {
    pub query: String,
    pub collection: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub hybrid_search: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestRetrieveRequest {
    pub url: String,
    pub collection: Option<String>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestEvaluateRequest {
    pub question: String,
    pub collection: Option<String>,
    pub diagnostics: Option<bool>,
    pub retrieval_ab: Option<bool>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub hybrid_search: Option<bool>,
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
            since: None,
            before: None,
            hybrid_search: None,
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
            since: None,
            before: None,
            max_points: req.max_points,
            cursor: req.cursor,
            token_budget: req.token_budget,
        }
    }
}
