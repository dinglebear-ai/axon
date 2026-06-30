# Trait Contract
Last Modified: 2026-06-30

## Contract

Traits are executable boundaries owned by domain crates. They isolate provider
choice, source-specific behavior, durable state, and test fakes.

## Rules

- Traits are `Send + Sync`.
- Async traits use `async_trait` or native async traits once stable enough.
- Every trait returns structured `ApiError`-compatible errors.
- Every trait has a fake implementation for tests.
- Traits accept/return DTOs from `axon-api`.
- Traits do not accept CLI/MCP/REST transport structs.

## Source and Routing Traits

```rust
#[async_trait]
pub trait SourceResolver: Send + Sync {
    async fn resolve(&self, request: &SourceRequest) -> Result<ResolvedSource>;
    async fn capabilities(&self) -> Result<SourceResolverCapability>;
}

#[async_trait]
pub trait SourceRouter: Send + Sync {
    async fn route(&self, source: ResolvedSource, request: &SourceRequest) -> Result<RoutePlan>;
    async fn validate_options(&self, plan: &RoutePlan) -> Result<ValidatedOptions>;
    async fn capabilities(&self) -> Result<SourceRouterCapability>;
}

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    async fn capabilities(&self) -> Result<SourceAdapterCapability>;
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest>;
    async fn acquire(&self, plan: &SourcePlan, diff: &SourceManifestDiff)
        -> Result<SourceAcquisition>;
    async fn normalize(&self, plan: &SourcePlan, acquisition: SourceAcquisition)
        -> Result<StageExecutionResult<Vec<SourceDocument>>>;
}
```

Adapters do not resolve raw source strings and do not choose execution plans.
`SourceResolver` resolves identity, `SourceRouter` selects adapter/scope/provider
requirements, and `SourceAdapter` discovers, acquires, and normalizes items for
the selected `SourcePlan`.

## Document and Parse Traits

```rust
#[async_trait]
pub trait SourceEnricher: Send + Sync {
    async fn enrich(&self, plan: &SourcePlan, item: &AcquiredSourceItem)
        -> Result<SourceEnrichment>;
    async fn capabilities(&self) -> Result<SourceEnricherCapability>;
}

#[async_trait]
pub trait DocumentPreparer: Send + Sync {
    async fn prepare(&self, document: SourceDocument) -> Result<PreparedDocument>;
    async fn prepare_many(&self, documents: Vec<SourceDocument>) -> Result<Vec<PreparedDocument>>;
    async fn capabilities(&self) -> Result<DocumentPreparerCapability>;
}

pub trait ChunkRouter: Send + Sync {
    fn route(&self, document: &SourceDocument) -> Result<ChunkProfile>;
    fn supported_profiles(&self) -> Vec<ChunkProfileCapability>;
}

#[async_trait]
pub trait Parser: Send + Sync {
    fn parser_id(&self) -> &'static str;
    fn supports(&self, document: &SourceDocument) -> bool;
    async fn parse(&self, document: &SourceDocument) -> Result<ParseResult>;
    async fn capabilities(&self) -> Result<ParserCapability>;
}
```

## Retrieval and Publish Traits

```rust
#[async_trait]
pub trait RetrievalEngine: Send + Sync {
    async fn query(&self, request: QueryRequest) -> Result<QueryResult>;
    async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult>;
    async fn build_ask_context(&self, request: AskRequest) -> Result<AskContext>;
    async fn capabilities(&self) -> Result<RetrievalCapability>;
}

#[async_trait]
pub trait GenerationPublisher: Send + Sync {
    async fn validate_publish(&self, request: PublishGenerationRequest) -> Result<PublishPlan>;
    async fn publish_generation(&self, request: PublishGenerationRequest)
        -> Result<PublishGenerationResult>;
}
```

## Fake Requirements

Every trait fake must support:

- deterministic success
- deterministic failure
- degraded/warning mode where applicable
- recorded calls for assertions
- capability override

## Completion Checklist

- every trait has a concrete owner crate
- every trait has production and fake implementations
- every trait has contract tests
- no transport crate imports concrete domain internals instead of traits/services
