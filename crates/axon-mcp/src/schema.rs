//! MCP transport contracts and action router.

pub use axon_api::action::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AxonRequest {
    Scrape(axon_api::source::ScrapeRequest),
    Crawl(axon_api::source::CrawlRequest),
    Embed(axon_api::source::EmbedRequest),
    Ingest(axon_api::source::IngestRequest),
    CodeSearch(axon_api::source::CodeSearchRequest),
    Status(StatusRequest),
    Jobs(JobsRequest),
    Extract(ExtractRequest),
    Memory(MemoryRequest),
    Query(QueryRequest),
    Retrieve(RetrieveRequest),
    Search(SearchRequest),
    Map(MapRequest),
    Endpoints(EndpointsRequest),
    Evaluate(EvaluateRequest),
    Suggest(SuggestRequest),
    Doctor(DoctorRequest),
    Domains(DomainsRequest),
    Sources(SourcesRequest),
    Stats(StatsRequest),
    Help(HelpRequest),
    Research(ResearchRequest),
    Ask(AskRequest),
    Summarize(SummarizeRequest),
    Screenshot(ScreenshotRequest),
    Brand(BrandRequest),
    Debug(DebugRequest),
    Prune(PruneMcpRequest),
    Diff(DiffRequest),
    Migrate(MigrateRequest),
    Watch(WatchRequest),
    Setup(SetupRequest),
    Source(SourceRequest),
    Resolve(ResolveRequest),
    Capabilities(CapabilitiesRequest),
    Providers(ProvidersRequest),
    Graph(GraphRequest),
    Chat(ChatRequest),
    Codex(CodexRequest),
}

/// The MCP contract version this server implements. Mirrors the REST
/// contract's `contract_version` (`docs/pipeline-unification/surfaces/
/// rest-contract.md`, §Shared Response Envelope) so a caller correlating
/// REST and MCP responses for the same deployment sees the same value.
pub const MCP_CONTRACT_VERSION: &str = "2026-06-30";

/// `AxonToolResponse` is the MCP `axon` tool's response envelope. Per the
/// tool contract (`docs/pipeline-unification/surfaces/tool-contract.md`,
/// §Design Rules: "Return structured envelopes for every response"), it is
/// converging toward the same shared envelope shape REST uses
/// (`axon_api::source::SuccessEnvelope`) — `request_id`/`contract_version`
/// are populated on every response; `job`/`watch`/`artifacts`/`pagination`/
/// `trace` are populated only where the constructing handler already has
/// that data (most call sites do not yet thread it through — see WS-G
/// followups), so they stay optional/empty rather than fabricated.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AxonToolResponse {
    pub ok: bool,
    pub action: String,
    pub subaction: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub data: Value,
    /// Unique id for this response, generated once per call. Present on
    /// every response (unlike the REST envelope's `request_id`, which is
    /// assigned by transport middleware, this one is assigned here because
    /// MCP has no equivalent per-request middleware seam yet).
    pub request_id: String,
    /// See [`MCP_CONTRACT_VERSION`].
    pub contract_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<Value>,
}

impl AxonToolResponse {
    pub fn ok(action: &str, subaction: &str, data: Value) -> Self {
        Self {
            ok: true,
            action: action.into(),
            subaction: subaction.into(),
            warnings: Vec::new(),
            data,
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            contract_version: MCP_CONTRACT_VERSION.into(),
            job: None,
            watch: None,
            artifacts: Vec::new(),
            pagination: None,
            trace: None,
        }
    }
    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
    pub fn with_job(mut self, job: Value) -> Self {
        self.job = Some(job);
        self
    }
    pub fn with_watch(mut self, watch: Value) -> Self {
        self.watch = Some(watch);
        self
    }
    pub fn with_artifact(mut self, artifact: Value) -> Self {
        self.artifacts.push(artifact);
        self
    }
    pub fn with_pagination(mut self, pagination: Value) -> Self {
        self.pagination = Some(pagination);
        self
    }
}

pub fn parse_axon_request(raw: Map<String, Value>) -> Result<AxonRequest, String> {
    if let Some(action) = raw.get("action").and_then(Value::as_str)
        && let Some(guidance) = removed_action_guidance(action)
    {
        return Err(format!(
            "action `{action}` was removed from MCP; {guidance}"
        ));
    }
    serde_json::from_value(Value::Object(raw)).map_err(|e| format!("invalid request shape: {e}"))
}

fn removed_action_guidance(action: &str) -> Option<&'static str> {
    match action {
        "vertical_scrape" => Some("use action=scrape"),
        "purge" | "dedupe" => Some("use action=prune"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
