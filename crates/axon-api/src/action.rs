#[path = "action/prune_request.rs"]
mod prune_request;
#[path = "action/requests.rs"]
mod requests;
#[path = "action/utility.rs"]
mod utility;
pub use prune_request::*;
pub use requests::*;
pub use utility::*;

/// Transport-neutral action request used by the local client/server bridge.
/// MCP owns its separate wire router in `axon-mcp::schema::AxonRequest`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActionRequest {
    Scrape(crate::source::ScrapeRequest),
    Crawl(crate::source::CrawlRequest),
    Embed(crate::source::EmbedRequest),
    Ingest(crate::source::IngestRequest),
    CodeSearch(crate::source::CodeSearchRequest),
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
