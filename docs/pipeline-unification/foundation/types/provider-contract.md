# Provider Type Contract
Last Modified: 2026-06-30

## Contract

Providers cross process, network, model, or rate-limit boundaries. Providers
perform one class of work and report capabilities, health, limits, and cooling.
Global fairness and scheduling belong to jobs/provider reservations, not to the
provider implementation.

## Required Provider Traits

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse>;
    async fn complete_streaming(
        &self,
        request: LlmCompletionRequest,
        on_delta: LlmDeltaSink,
    ) -> Result<LlmCompletionResponse>;
    async fn capabilities(&self) -> Result<LlmProviderCapability>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult>;
    async fn capabilities(&self) -> Result<EmbeddingProviderCapability>;
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn capabilities(&self) -> Result<VectorStoreCapability>;
}

#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult>;
    async fn capabilities(&self) -> Result<SearchProviderCapability>;
}

#[async_trait]
pub trait FetchProvider: Send + Sync {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource>;
    async fn capabilities(&self) -> Result<FetchProviderCapability>;
}

#[async_trait]
pub trait RenderProvider: Send + Sync {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource>;
    async fn capabilities(&self) -> Result<RenderProviderCapability>;
}

#[async_trait]
pub trait NetworkCaptureProvider: Send + Sync {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult>;
    async fn capabilities(&self) -> Result<NetworkCaptureProviderCapability>;
}

#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn resolve(&self, request: CredentialRequest) -> Result<CredentialMaterial>;
    async fn capabilities(&self) -> Result<CredentialProviderCapability>;
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, request: RateLimitRequest) -> Result<RateLimitPermit>;
    async fn capabilities(&self) -> Result<RateLimiterCapability>;
}

#[async_trait]
pub trait HealthProbe: Send + Sync {
    async fn probe(&self, request: HealthProbeRequest) -> Result<HealthReport>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait SecurityPolicy: Send + Sync {
    async fn authorize_source(&self, request: SourceRequest) -> Result<SecurityDecision>;
    async fn authorize_route(&self, request: RouteAuthRequest) -> Result<SecurityDecision>;
    async fn capabilities(&self) -> Result<SecurityPolicyCapability>;
}
```

Note: `HealthProbe` (`crates/axon-core/src/boundary.rs`) is the one trait
above that already uses the shared `ProviderCapability`/`HealthProbeRequest`/
`HealthReport` types instead of a bespoke per-trait request/result pair — the
other provider traits still use their own dedicated capability structs
(`LlmProviderCapability`, `EmbeddingProviderCapability`, etc., all real types
in `crates/axon-api/src/source/capability.rs`).

## Capability Requirements

Every provider capability includes:

- `provider_id`
- `provider_kind`
- `implementation`
- `version`
- `health`
- `limits`
- `features`
- `cooldown_until`
- redacted `last_error`

## Reservation Rules

- provider calls require scheduler reservations except health probes and fakes
- `EmbeddingProvider` does not own global concurrency
- `VectorStore` write capacity is separate from embedding capacity
- `LlmProvider` capacity is separate from embedding and vector capacity
- background source jobs cannot starve interactive query/ask work

## Fake Requirements

Every provider fake supports:

- deterministic success
- deterministic timeout
- deterministic rate limit
- deterministic fatal error
- capability override
- health override
- call recording

## Completion Checklist

- every provider has capability tests
- every provider has health/cooling tests
- every provider has a fake
- no provider owns source ledger or transport rendering
