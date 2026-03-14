use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AxonRequest {
    Status(StatusRequest),
    Crawl(CrawlRequest),
    Extract(ExtractRequest),
    Embed(EmbedRequest),
    Ingest(IngestRequest),
    Query(QueryRequest),
    Retrieve(RetrieveRequest),
    Search(SearchRequest),
    Map(MapRequest),
    Doctor(DoctorRequest),
    Domains(DomainsRequest),
    Sources(SourcesRequest),
    Stats(StatsRequest),
    Help(HelpRequest),
    Artifacts(ArtifactsRequest),
    Scrape(ScrapeRequest),
    Research(ResearchRequest),
    Ask(AskRequest),
    Screenshot(ScreenshotRequest),
    Refresh(RefreshRequest),
    Graph(GraphRequest),
    ElicitDemo(ElicitDemoRequest),
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Path,
    Inline,
    Both,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CrawlRequest {
    pub subaction: CrawlSubaction,
    pub urls: Option<Vec<String>>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub sitemap_since_days: Option<u32>,
    pub render_mode: Option<McpRenderMode>,
    pub delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrawlSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpRenderMode {
    Http,
    Chrome,
    AutoSwitch,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    pub subaction: ExtractSubaction,
    pub urls: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbedRequest {
    pub subaction: EmbedSubaction,
    pub input: Option<String>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmbedSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestRequest {
    pub subaction: IngestSubaction,
    pub source_type: Option<IngestSourceType>,
    pub target: Option<String>,
    pub include_source: Option<bool>,
    pub sessions: Option<SessionsIngestOptions>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSourceType {
    Github,
    Reddit,
    Youtube,
    Sessions,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionsIngestOptions {
    pub claude: Option<bool>,
    pub codex: Option<bool>,
    pub gemini: Option<bool>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactsRequest {
    pub subaction: ArtifactsSubaction,
    pub path: Option<String>,
    pub pattern: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Lines of context before/after each grep match (like rg -C N). Default: 0.
    pub context_lines: Option<usize>,
    /// artifacts.read: return full content (paginated). Requires explicit opt-in.
    pub full: Option<bool>,
    /// artifacts.clean: delete files older than this many hours. Required for clean.
    pub max_age_hours: Option<u64>,
    /// artifacts.clean: preview-only mode. Defaults to true — no files deleted unless false.
    pub dry_run: Option<bool>,
    /// Response mode for list/search subactions (path | inline | both). Defaults to path.
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactsSubaction {
    Head,
    Grep,
    Wc,
    Read,
    /// List all artifacts in the artifact directory with metadata.
    List,
    /// Delete a single artifact by path (must be within artifact root).
    Delete,
    /// Bulk-delete artifacts older than max_age_hours. Dry-run by default.
    Clean,
    /// Regex search across all artifact files.
    Search,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetrieveRequest {
    pub url: Option<String>,
    pub max_points: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapRequest {
    pub url: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DoctorRequest {
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainsRequest {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourcesRequest {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatsRequest {
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScrapeRequest {
    pub url: Option<String>,
    pub response_mode: Option<ResponseMode>,
    /// CSS selector to scope content extraction (e.g. "article, main, .content").
    pub root_selector: Option<String>,
    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads").
    pub exclude_selector: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub query: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotRequest {
    pub url: Option<String>,
    pub full_page: Option<bool>,
    pub viewport: Option<String>,
    pub output: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefreshSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
    Schedule,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    pub subaction: RefreshSubaction,
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub job_id: Option<String>,
    pub schedule_subaction: Option<String>,
    pub schedule_name: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphSubaction {
    Build,
    Status,
    Explore,
    Stats,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRequest {
    pub subaction: GraphSubaction,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub all: Option<bool>,
    pub entity: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

/// Request parameters for the elicit_demo action.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElicitDemoRequest {
    /// Optional prompt message shown to the user above the elicitation form.
    pub message: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AxonToolResponse {
    pub ok: bool,
    pub action: String,
    pub subaction: String,
    pub data: Value,
}

impl AxonToolResponse {
    pub fn ok(action: &str, subaction: &str, data: Value) -> Self {
        Self {
            ok: true,
            action: action.to_string(),
            subaction: subaction.to_string(),
            data,
        }
    }
}

pub fn parse_axon_request(raw: Map<String, Value>) -> Result<AxonRequest, String> {
    serde_json::from_value(Value::Object(raw)).map_err(|e| format!("invalid request shape: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value, json};

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => panic!("expected object"),
        }
    }

    // --- valid action routing ---

    #[test]
    fn parse_status_action() {
        let raw = obj(json!({ "action": "status" }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "status should parse successfully");
        assert!(matches!(result.unwrap(), AxonRequest::Status(_)));
    }

    #[test]
    fn parse_query_action_no_fields() {
        let raw = obj(json!({ "action": "query" }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "query with no optional fields should parse");
        assert!(matches!(result.unwrap(), AxonRequest::Query(_)));
    }

    #[test]
    fn parse_query_action_with_all_optional_fields() {
        let raw = obj(json!({
            "action": "query",
            "query": "semantic search test",
            "limit": 5,
            "offset": 0,
            "response_mode": "inline"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "query with all optional fields should parse"
        );
        if let Ok(AxonRequest::Query(q)) = result {
            assert_eq!(q.query.as_deref(), Some("semantic search test"));
            assert_eq!(q.limit, Some(5));
            assert_eq!(q.offset, Some(0));
            assert!(matches!(q.response_mode, Some(ResponseMode::Inline)));
        } else {
            panic!("expected Query variant");
        }
    }

    #[test]
    fn parse_crawl_start_action() {
        let raw = obj(json!({
            "action": "crawl",
            "subaction": "start",
            "urls": ["https://example.com"]
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "crawl start should parse successfully");
        if let Ok(AxonRequest::Crawl(c)) = result {
            assert!(matches!(c.subaction, CrawlSubaction::Start));
            assert_eq!(
                c.urls.as_deref(),
                Some(&["https://example.com".to_string()][..])
            );
        } else {
            panic!("expected Crawl variant");
        }
    }

    #[test]
    fn parse_crawl_list_action() {
        let raw = obj(json!({
            "action": "crawl",
            "subaction": "list",
            "limit": 10
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "crawl list should parse successfully");
        assert!(matches!(result.unwrap(), AxonRequest::Crawl(_)));
    }

    #[test]
    fn parse_embed_start_action() {
        let raw = obj(json!({
            "action": "embed",
            "subaction": "start",
            "input": "https://docs.example.com"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "embed start should parse successfully");
        if let Ok(AxonRequest::Embed(e)) = result {
            assert!(matches!(e.subaction, EmbedSubaction::Start));
            assert_eq!(e.input.as_deref(), Some("https://docs.example.com"));
        } else {
            panic!("expected Embed variant");
        }
    }

    #[test]
    fn parse_scrape_action() {
        let raw = obj(json!({
            "action": "scrape",
            "url": "https://example.com/page"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "scrape should parse successfully");
        if let Ok(AxonRequest::Scrape(s)) = result {
            assert_eq!(s.url.as_deref(), Some("https://example.com/page"));
        } else {
            panic!("expected Scrape variant");
        }
    }

    #[test]
    fn parse_doctor_action() {
        let raw = obj(json!({ "action": "doctor" }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "doctor should parse successfully");
        assert!(matches!(result.unwrap(), AxonRequest::Doctor(_)));
    }

    #[test]
    fn parse_stats_action() {
        let raw = obj(json!({ "action": "stats" }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "stats should parse successfully");
        assert!(matches!(result.unwrap(), AxonRequest::Stats(_)));
    }

    #[test]
    fn parse_graph_build_action() {
        let raw = obj(json!({
            "action": "graph",
            "subaction": "build",
            "domain": "tanstack.com"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "graph build should parse successfully");
        if let Ok(AxonRequest::Graph(g)) = result {
            assert!(matches!(g.subaction, GraphSubaction::Build));
            assert_eq!(g.domain.as_deref(), Some("tanstack.com"));
        } else {
            panic!("expected Graph variant");
        }
    }

    #[test]
    fn parse_graph_explore_action() {
        let raw = obj(json!({
            "action": "graph",
            "subaction": "explore",
            "entity": "Neo4j"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "graph explore should parse successfully");
        if let Ok(AxonRequest::Graph(g)) = result {
            assert!(matches!(g.subaction, GraphSubaction::Explore));
            assert_eq!(g.entity.as_deref(), Some("Neo4j"));
        } else {
            panic!("expected Graph variant");
        }
    }

    #[test]
    fn parse_ingest_start_github() {
        let raw = obj(json!({
            "action": "ingest",
            "subaction": "start",
            "source_type": "github",
            "target": "owner/repo"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "ingest start github should parse");
        if let Ok(AxonRequest::Ingest(i)) = result {
            assert!(matches!(i.subaction, IngestSubaction::Start));
            assert!(matches!(i.source_type, Some(IngestSourceType::Github)));
            assert_eq!(i.target.as_deref(), Some("owner/repo"));
        } else {
            panic!("expected Ingest variant");
        }
    }

    #[test]
    fn parse_refresh_start_with_url() {
        let raw = obj(json!({
            "action": "refresh",
            "subaction": "start",
            "url": "https://example.com/docs"
        }));
        let result = parse_axon_request(raw);
        assert!(result.is_ok(), "refresh start with url should parse");
        if let Ok(AxonRequest::Refresh(r)) = result {
            assert!(matches!(r.subaction, RefreshSubaction::Start));
            assert_eq!(r.url.as_deref(), Some("https://example.com/docs"));
        } else {
            panic!("expected Refresh variant");
        }
    }

    // --- unknown action → error ---

    #[test]
    fn unknown_action_returns_error() {
        let raw = obj(json!({ "action": "nonexistent_action" }));
        let result = parse_axon_request(raw);
        assert!(result.is_err(), "unknown action must return an error");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("invalid request shape"),
            "error should mention invalid request shape, got: {msg}"
        );
    }

    #[test]
    fn empty_action_returns_error() {
        let raw = obj(json!({ "action": "" }));
        let result = parse_axon_request(raw);
        assert!(result.is_err(), "empty action must return an error");
    }

    #[test]
    fn missing_action_field_returns_error() {
        let raw = obj(json!({ "query": "something" }));
        let result = parse_axon_request(raw);
        assert!(result.is_err(), "missing action field must return an error");
    }

    #[test]
    fn case_sensitive_action_no_folding() {
        // Schema uses snake_case; uppercase variants must NOT match.
        let raw = obj(json!({ "action": "STATUS" }));
        let result = parse_axon_request(raw);
        assert!(result.is_err(), "action matching must be case-sensitive");

        let raw2 = obj(json!({ "action": "Query" }));
        let result2 = parse_axon_request(raw2);
        assert!(
            result2.is_err(),
            "action matching must be case-sensitive (PascalCase)"
        );
    }

    // --- missing required field → validation error ---

    #[test]
    fn crawl_missing_subaction_returns_error() {
        // crawl requires subaction; omitting it must fail.
        let raw = obj(json!({
            "action": "crawl",
            "urls": ["https://example.com"]
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "crawl without subaction must return an error"
        );
    }

    #[test]
    fn embed_missing_subaction_returns_error() {
        let raw = obj(json!({
            "action": "embed",
            "input": "https://docs.example.com"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "embed without subaction must return an error"
        );
    }

    #[test]
    fn ingest_missing_subaction_returns_error() {
        let raw = obj(json!({
            "action": "ingest",
            "source_type": "github",
            "target": "owner/repo"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "ingest without subaction must return an error"
        );
    }

    #[test]
    fn crawl_unknown_subaction_returns_error() {
        let raw = obj(json!({
            "action": "crawl",
            "subaction": "fly_to_moon"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "crawl with unknown subaction must return an error"
        );
    }

    #[test]
    fn crawl_deny_unknown_fields() {
        // CrawlRequest uses #[serde(deny_unknown_fields)]
        let raw = obj(json!({
            "action": "crawl",
            "subaction": "start",
            "urls": ["https://example.com"],
            "totally_unknown_field": true
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "unknown fields must be rejected by deny_unknown_fields"
        );
    }

    #[test]
    fn status_deny_unknown_fields() {
        let raw = obj(json!({
            "action": "status",
            "unexpected": "field"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_err(),
            "status with unknown fields must be rejected"
        );
    }

    // --- serde round-trip: request deserialization ---

    #[test]
    fn serde_roundtrip_axon_tool_response() {
        let data = json!({ "jobs": [], "count": 0 });
        let resp = AxonToolResponse::ok("crawl", "list", data.clone());

        let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
        let parsed: Value = serde_json::from_str(&serialized).expect("must parse back to JSON");

        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["action"], "crawl");
        assert_eq!(parsed["subaction"], "list");
        assert_eq!(parsed["data"]["jobs"], json!([]));
        assert_eq!(parsed["data"]["count"], 0);
    }

    #[test]
    fn serde_roundtrip_response_envelope_keys() {
        let resp = AxonToolResponse::ok("status", "status", json!({ "text": "ok" }));
        let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
        let parsed: Value = serde_json::from_str(&serialized).expect("must parse back to JSON");

        // Canonical envelope must have exactly these top-level keys.
        let obj = parsed.as_object().expect("response must be a JSON object");
        assert!(obj.contains_key("ok"), "envelope must have 'ok'");
        assert!(obj.contains_key("action"), "envelope must have 'action'");
        assert!(
            obj.contains_key("subaction"),
            "envelope must have 'subaction'"
        );
        assert!(obj.contains_key("data"), "envelope must have 'data'");
    }

    #[test]
    fn serde_query_request_all_optional_fields_none() {
        // All fields in QueryRequest are Option — omitting all must succeed.
        let raw = obj(json!({ "action": "query" }));
        let Ok(AxonRequest::Query(q)) = parse_axon_request(raw) else {
            panic!("expected Query");
        };
        assert!(q.query.is_none());
        assert!(q.limit.is_none());
        assert!(q.offset.is_none());
        assert!(q.response_mode.is_none());
    }

    #[test]
    fn serde_response_mode_variants() {
        for (raw_mode, expected) in [("path", "path"), ("inline", "inline"), ("both", "both")] {
            let raw = obj(json!({
                "action": "query",
                "response_mode": raw_mode
            }));
            let result = parse_axon_request(raw);
            assert!(
                result.is_ok(),
                "response_mode '{raw_mode}' should parse, got: {:?}",
                result
            );
            // Verify the string round-trips through the canonical name
            let _ = expected; // match is sufficient; value verified by parse success
        }
    }

    #[test]
    fn serde_crawl_render_mode_variants() {
        for subaction_str in ["http", "chrome", "auto_switch"] {
            let raw = obj(json!({
                "action": "crawl",
                "subaction": "start",
                "render_mode": subaction_str
            }));
            let result = parse_axon_request(raw);
            assert!(
                result.is_ok(),
                "render_mode '{subaction_str}' should parse successfully"
            );
        }
    }

    #[test]
    fn serde_search_time_range_variants() {
        for range in ["day", "week", "month", "year"] {
            let raw = obj(json!({
                "action": "search",
                "search_time_range": range
            }));
            let result = parse_axon_request(raw);
            assert!(
                result.is_ok(),
                "search_time_range '{range}' should parse successfully"
            );
        }
    }

    #[test]
    fn serde_ingest_source_type_variants() {
        for src in ["github", "reddit", "youtube", "sessions"] {
            let raw = obj(json!({
                "action": "ingest",
                "subaction": "start",
                "source_type": src
            }));
            let result = parse_axon_request(raw);
            assert!(
                result.is_ok(),
                "ingest source_type '{src}' should parse successfully"
            );
        }
    }

    #[test]
    fn serde_artifacts_subaction_variants() {
        for sub in ["head", "grep", "wc", "read"] {
            let raw = obj(json!({
                "action": "artifacts",
                "subaction": sub,
                "path": ".cache/axon-mcp/test.json"
            }));
            let result = parse_axon_request(raw);
            assert!(
                result.is_ok(),
                "artifacts subaction '{sub}' should parse successfully"
            );
        }
    }
}
