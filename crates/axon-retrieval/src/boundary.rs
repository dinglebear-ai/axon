//! Retrieval boundary trait — target contract for `RetrievalEngine`.
//!
//! Contract: `docs/pipeline-unification/foundation/types/trait-contract.md`
//! §RetrievalEngine. This trait is defined in a module separate from
//! `crate::engine::RetrievalEngine` because the concrete struct and the trait
//! share the bare name `RetrievalEngine` — Rust's type namespace forbids a
//! struct and a trait of the identical name in the same module.
//!
//! Non-breaking compatibility note: `impl RetrievalEngine for
//! crate::engine::RetrievalEngine<S, E>` implements `retrieve` by calling
//! `self.retrieve(request).await`. That call resolves to the pre-existing
//! **inherent** `retrieve` method (identical signature) rather than
//! recursing into this trait method — inherent methods always take priority
//! over trait methods during dot-call resolution for a given receiver type,
//! even from within the trait impl body that defines the trait method. This
//! is the load-bearing trick that lets every existing direct caller of
//! `RetrievalEngine::retrieve` (e.g. `crate::service::run_query`) keep
//! compiling unchanged while the async-trait-shaped method becomes reachable
//! through `&dyn RetrievalEngine` / generic-bound dispatch.

use async_trait::async_trait;
use axon_api::mcp_schema::AskRequest;
use axon_api::source::{ApiError, CapabilityBase, HealthStatus, MetadataMap, RetrievalCapability};

use crate::ask_context::AskContext;
use crate::memory::MEMORY_SOURCE_KIND;
use crate::query::{QueryRequest, QueryResult, RetrievalRequest, RetrievalResult};

pub const MODULE_NAME: &str = "boundary";

pub type Result<T> = std::result::Result<T, ApiError>;

/// Default context byte budget for `query`/`build_ask_context` composition
/// through this boundary. Mirrors `crate::service::DEFAULT_BYTE_BUDGET`
/// (2 MiB) — generous enough that the context-budget guard never trims
/// legitimate results for the default (non-`ask`-tuned) path.
const DEFAULT_BYTE_BUDGET: u64 = 2 * 1024 * 1024;
/// Default context token budget, paired with [`DEFAULT_BYTE_BUDGET`].
const DEFAULT_TOKEN_BUDGET: u32 = u32::MAX;
/// Default chunk limit for `build_ask_context` when the caller's
/// `AskRequest` does not set `ask_chunk_limit`.
const DEFAULT_ASK_CHUNK_LIMIT: u32 = 12;

#[async_trait]
pub trait RetrievalEngine: Send + Sync {
    async fn query(&self, request: QueryRequest) -> Result<QueryResult>;
    async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult>;
    async fn build_ask_context(&self, request: AskRequest) -> Result<AskContext>;
    async fn capabilities(&self) -> Result<RetrievalCapability>;
}

#[async_trait]
impl<S, E> RetrievalEngine for crate::engine::RetrievalEngine<S, E>
where
    S: axon_vectors::store::VectorStore + 'static,
    E: axon_embedding::provider::EmbeddingProvider + 'static,
{
    async fn query(&self, request: QueryRequest) -> Result<QueryResult> {
        let retrieval_request = RetrievalRequest {
            query: request.query,
            collection: request.collection,
            limit: request.limit.max(1),
            source_id: None,
            generation: None,
            namespace_filters: request.namespace_filters,
            excluded_source_kinds: vec![MEMORY_SOURCE_KIND.to_string()],
            byte_budget: DEFAULT_BYTE_BUDGET,
            token_budget: DEFAULT_TOKEN_BUDGET,
        };
        let result = self.retrieve(retrieval_request).await?;
        Ok(QueryResult {
            matches: result.matches,
            citations: result.citations,
        })
    }

    async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult> {
        // Direct passthrough to the inherent method — see module doc comment.
        self.retrieve(request).await
    }

    async fn build_ask_context(&self, request: AskRequest) -> Result<AskContext> {
        let limit = request
            .ask_chunk_limit
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(DEFAULT_ASK_CHUNK_LIMIT);
        let byte_budget = request
            .ask_max_context_chars
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(DEFAULT_BYTE_BUDGET);
        let retrieval_request = RetrievalRequest {
            query: request.query.unwrap_or_default(),
            collection: request.collection.unwrap_or_else(|| "axon".to_string()),
            limit,
            source_id: None,
            generation: None,
            namespace_filters: Vec::new(),
            excluded_source_kinds: vec![MEMORY_SOURCE_KIND.to_string()],
            byte_budget,
            token_budget: DEFAULT_TOKEN_BUDGET,
        };
        let retrieval = self.retrieve(retrieval_request).await?;
        Ok(AskContext {
            context: retrieval.context.clone(),
            citations: retrieval.citations.clone(),
            retrieval,
        })
    }

    async fn capabilities(&self) -> Result<RetrievalCapability> {
        Ok(RetrievalCapability(CapabilityBase {
            name: "retrieval_engine".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-retrieval".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["hybrid_search".to_string(), "bm42_sparse".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
