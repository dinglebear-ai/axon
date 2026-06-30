# Service Contract
Last Modified: 2026-06-30

## Contract

Services are `axon-services` orchestration entry points. They compose traits,
stores, providers, and DTOs into product use cases. Services do not parse
transport-specific input and do not reach into domain internals.

## Rules

- Services accept `axon-api` request DTOs or narrow service request DTOs.
- Services return `axon-api` result DTOs.
- Services are the only layer called directly by CLI, MCP, and REST.
- Services do not build vector payloads, parse documents, or fetch sources
  directly when a boundary trait exists.
- Every service has fake-backed integration tests.

## Required Service Traits

```rust
#[async_trait]
pub trait SourceService: Send + Sync {
    async fn submit(&self, request: SourceRequest) -> Result<SourceResult>;
    async fn run_now(&self, request: SourceRequest) -> Result<SourceResult>;
    async fn resolve(&self, request: SourceRequest) -> Result<ResolvedSource>;
    async fn get(&self, source_id: SourceId) -> Result<SourceSummary>;
    async fn list(&self, request: SourceListRequest) -> Result<Page<SourceSummary>>;
    async fn items(&self, request: SourceItemListRequest) -> Result<Page<SourceItem>>;
    async fn generations(&self, request: SourceGenerationListRequest)
        -> Result<Page<SourceGenerationSummary>>;
}

#[async_trait]
pub trait WatchService: Send + Sync {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult>;
    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
    async fn get(&self, watch_id: WatchId) -> Result<WatchResult>;
    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>>;
    async fn exec(&self, watch_id: WatchId, request: WatchExecRequest) -> Result<JobDescriptor>;
    async fn pause(&self, watch_id: WatchId) -> Result<WatchResult>;
    async fn resume(&self, watch_id: WatchId) -> Result<WatchResult>;
    async fn delete(&self, watch_id: WatchId) -> Result<DeleteResult>;
    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
}

#[async_trait]
pub trait JobService: Send + Sync {
    async fn get(&self, job_id: JobId) -> Result<JobSummary>;
    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage>;
    async fn cancel(&self, job_id: JobId) -> Result<JobSummary>;
    async fn retry(&self, job_id: JobId) -> Result<JobDescriptor>;
    async fn recover(&self, request: JobRecoverRequest) -> Result<JobRecoverResult>;
    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult>;
    async fn clear(&self, request: JobClearRequest) -> Result<JobClearResult>;
}

#[async_trait]
pub trait QueryService: Send + Sync {
    async fn query(&self, request: QueryRequest) -> Result<QueryResult>;
}

#[async_trait]
pub trait RetrieveService: Send + Sync {
    async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult>;
}

#[async_trait]
pub trait AskService: Send + Sync {
    async fn ask(&self, request: AskRequest) -> Result<AskResult>;
    async fn chat(&self, request: ChatRequest) -> Result<ChatResult>;
    async fn evaluate(&self, request: EvaluationRequest) -> Result<EvaluationResult>;
    async fn suggest(&self, request: SuggestRequest) -> Result<SuggestResult>;
}

#[async_trait]
pub trait ExtractService: Send + Sync {
    async fn extract(&self, request: ExtractRequest) -> Result<ExtractResult>;
    async fn summarize(&self, request: SummarizeRequest) -> Result<SummarizeResult>;
    async fn research(&self, request: ResearchRequest) -> Result<ResearchResult>;
}

#[async_trait]
pub trait MemoryService: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> Result<MemoryRecord>;
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn forget(&self, memory_id: MemoryId) -> Result<MemoryResult>;
}

#[async_trait]
pub trait GraphService: Send + Sync {
    async fn kinds(&self) -> Result<GraphKindDocument>;
    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult>;
    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> Result<GraphNode>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<GraphEdge>;
}

#[async_trait]
pub trait DocumentService: Send + Sync {
    async fn list(&self, request: DocumentListRequest) -> Result<Page<DocumentSummary>>;
    async fn get(&self, document_id: DocumentId) -> Result<DocumentDetail>;
    async fn chunks(&self, request: ChunkListRequest) -> Result<Page<ChunkSummary>>;
    async fn chunk(&self, request: ChunkGetRequest) -> Result<ChunkDetail>;
}

#[async_trait]
pub trait PruneService: Send + Sync {
    async fn plan(&self, request: PruneRequest) -> Result<PrunePlan>;
    async fn execute(&self, request: PruneExecuteRequest) -> Result<PruneResult>;
    async fn dedupe(&self, request: DedupeRequest) -> Result<DedupeResult>;
    async fn cleanup_debt(&self, request: CleanupDebtRequest) -> Result<CleanupDebtResult>;
}

#[async_trait]
pub trait ProviderService: Send + Sync {
    async fn capabilities(&self) -> Result<CapabilityDocument>;
    async fn providers(&self) -> Result<Vec<ProviderSummary>>;
    async fn provider(&self, provider_id: ProviderId) -> Result<ProviderCapability>;
    async fn health(&self) -> Result<HealthReport>;
    async fn doctor(&self) -> Result<DoctorReport>;
}

#[async_trait]
pub trait CollectionService: Send + Sync {
    async fn list(&self) -> Result<Vec<CollectionSummary>>;
    async fn get(&self, collection: String) -> Result<CollectionSpec>;
    async fn ensure(&self, spec: CollectionSpec) -> Result<CollectionSpec>;
    async fn delete(&self, collection: String) -> Result<DeleteResult>;
}

#[async_trait]
pub trait ResetService: Send + Sync {
    async fn plan(&self, request: ResetRequest) -> Result<ResetPlan>;
    async fn execute(&self, request: ResetExecuteRequest) -> Result<ResetResult>;
}
```

## Completion Checklist

- every service has CLI/MCP/REST parity tests where exposed
- every service has fake-backed tests
- no service calls provider/client internals directly when a boundary exists
- no transport bypasses services
