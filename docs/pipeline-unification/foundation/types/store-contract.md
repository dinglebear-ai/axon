# Store Contract
Last Modified: 2026-06-30

## Contract

Stores own durable state. They provide transaction, lease, reset, capability,
and fake implementations. Stores are not provider clients unless explicitly
noted, and stores do not parse transport input.

## Rules

- Every store has SQLite or filesystem production implementation unless stated
  otherwise.
- Every store has an in-memory fake.
- Store methods are idempotent where retryable.
- Store errors are structured and retry-aware.
- Reset is explicit and destructive; there is no old-data migration contract.

## Required Store Traits

```rust
#[async_trait]
pub trait LedgerStore: Send + Sync {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()>;
    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>>;
    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()>;
    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff>;
    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration>;
    async fn publish_generation(&self, generation: SourceGeneration) -> Result<()>;
    async fn update_document_status(&self, status: DocumentStatus) -> Result<()>;
    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()>;
    async fn capabilities(&self) -> Result<LedgerStoreCapability>;
}

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>>;
    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult>;
    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult>;
    async fn capabilities(&self) -> Result<GraphStoreCapability>;
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>>;
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn reinforce(&self, memory_id: MemoryId, signal: MemoryReinforcement) -> Result<MemoryResult>;
    async fn capabilities(&self) -> Result<MemoryStoreCapability>;
}

#[async_trait]
pub trait JobStore: Send + Sync {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor>;
    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>>;
    async fn update_status(&self, status: JobStatusUpdate) -> Result<()>;
    async fn append_event(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage>;
    async fn capabilities(&self) -> Result<JobStoreCapability>;
}

#[async_trait]
pub trait WatchStore: Send + Sync {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult>;
    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>>;
    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>>;
    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()>;
    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
    async fn capabilities(&self) -> Result<WatchStoreCapability>;
}

#[async_trait]
pub trait ArtifactStore: Send + Sync {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle>;
    async fn get(&self, handle: ArtifactHandle) -> Result<ArtifactReadResult>;
    async fn delete(&self, handle: ArtifactHandle) -> Result<()>;
    async fn capabilities(&self) -> Result<ArtifactStoreCapability>;
}

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn load(&self) -> Result<EffectiveConfig>;
    async fn validate(&self) -> Result<ConfigValidationReport>;
    async fn snapshot(&self) -> Result<ConfigSnapshotId>;
    async fn capabilities(&self) -> Result<ConfigStoreCapability>;
}

#[async_trait]
pub trait DocumentCache: Send + Sync {
    async fn get(&self, key: DocumentCacheKey) -> Result<Option<CachedDocument>>;
    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> Result<()>;
    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> Result<()>;
    async fn capabilities(&self) -> Result<DocumentCacheCapability>;
}
```

## Transaction Rules

- generation publish is transactional from the user's perspective
- cleanup debt creation is durable before reporting completion
- vector deletes are driven by cleanup debt, not hidden Qdrant scans
- reset can drop stores because cutover assumes empty stores

## Completion Checklist

- every store has production and fake implementation
- every store has reset tests
- every store has transaction/idempotency tests
- every store reports capability and health
