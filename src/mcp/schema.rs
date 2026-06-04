use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[path = "schema/requests.rs"]
mod requests;
#[path = "schema/utility.rs"]
mod utility;
pub use requests::*;
pub use utility::*;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
    Endpoints(EndpointsRequest),
    Evaluate(EvaluateRequest),
    Suggest(SuggestRequest),
    Doctor(DoctorRequest),
    Domains(DomainsRequest),
    Sources(SourcesRequest),
    Stats(StatsRequest),
    Help(HelpRequest),
    Scrape(ScrapeRequest),
    Research(ResearchRequest),
    Ask(AskRequest),
    Summarize(SummarizeRequest),
    Screenshot(ScreenshotRequest),
    Brand(BrandRequest),
    Debug(DebugRequest),
    Dedupe(DedupeRequest),
    Diff(DiffRequest),
    Migrate(MigrateRequest),
    Watch(WatchRequest),
    Setup(SetupRequest),
    ElicitDemo(ElicitDemoRequest),
    VerticalScrape(VerticalScrapeRequest),
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AxonToolResponse {
    pub ok: bool,
    pub action: String,
    pub subaction: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub data: Value,
}

impl AxonToolResponse {
    pub fn ok(action: &str, subaction: &str, data: Value) -> Self {
        Self {
            ok: true,
            action: action.to_string(),
            subaction: subaction.to_string(),
            warnings: Vec::new(),
            data,
        }
    }

    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
}

pub fn parse_axon_request(raw: Map<String, Value>) -> Result<AxonRequest, String> {
    serde_json::from_value(Value::Object(raw)).map_err(|e| format!("invalid request shape: {e}"))
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
