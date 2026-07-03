//! Public retrieval service entry.
//!
//! This is the transport-neutral seam that runtime callers (CLI/MCP/REST via
//! `axon-services`) use to run semantic `query` through the new
//! [`RetrievalEngine`] instead of the legacy `axon_vector` path. It exposes slim
//! public request/result DTOs and constructs the (crate-private) engine from a
//! runtime-held vector store + embedding provider.

use std::sync::Arc;

use axon_api::source::ProviderId;
use axon_embedding::provider::EmbeddingProvider;
use axon_error::ApiError;
use axon_vectors::store::VectorStore;

use crate::engine::{RetrievalAccess, RetrievalEngine, RetrievalEngineConfig};
use crate::query::RetrievalRequest;

pub const MODULE_NAME: &str = "service";

/// Default context byte budget for a plain semantic `query` (no LLM synthesis).
///
/// `query` returns per-chunk hits, not a fused context blob, so the budget only
/// needs to admit every returned chunk. Chosen generously (2 MiB) so the
/// context-budget guard never trims legitimate results.
const DEFAULT_BYTE_BUDGET: u64 = 2 * 1024 * 1024;
/// Default context token budget, paired with [`DEFAULT_BYTE_BUDGET`].
const DEFAULT_TOKEN_BUDGET: u32 = u32::MAX;

/// Slim public request for [`run_query`].
///
/// Only the fields a plain `query` needs are exposed; visibility defaults to the
/// standard read set (public/internal/derived) via [`RetrievalAccess::standard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryServiceRequest {
    pub query: String,
    pub collection: String,
    pub limit: u32,
}

/// One retrieval hit, mapped from the engine's internal match.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryServiceHit {
    /// Canonical URI of the chunk's document (`chunk_locator.canonical_uri`).
    pub canonical_uri: String,
    /// The chunk's own id.
    pub chunk_id: String,
    /// Vector search score.
    pub score: f64,
    /// Chunk text.
    pub text: String,
}

/// Result of [`run_query`].
#[derive(Debug, Clone, PartialEq)]
pub struct QueryServiceResult {
    pub hits: Vec<QueryServiceHit>,
}

/// Run a semantic `query` through the new [`RetrievalEngine`].
///
/// Constructs the engine from a runtime-held `store` + `provider` (trait objects
/// satisfy the generic bound via the `Arc<dyn _>` blanket impls in
/// `axon-vectors`/`axon-embedding`), issues a dense + bm42-sparse hybrid search,
/// and maps the engine's matches into transport-neutral hits.
pub async fn run_query(
    store: Arc<dyn VectorStore>,
    provider: Arc<dyn EmbeddingProvider>,
    embedding_provider_id: ProviderId,
    embedding_model: impl Into<String>,
    embedding_dimensions: u32,
    request: QueryServiceRequest,
) -> Result<QueryServiceResult, ApiError> {
    // Resolve the provider's *actual* identity from its capabilities. The
    // engine cross-checks the embed response's provider_id/model/dimensions, and
    // a provider (e.g. TEI) stamps its own id/model regardless of the runtime's
    // configured hint — so trust the provider, falling back to caller hints only
    // when capabilities omit the embedding block.
    let (provider_id, model, dimensions) = resolve_identity(
        provider.as_ref(),
        embedding_provider_id,
        embedding_model,
        embedding_dimensions,
    )
    .await?;
    let config =
        RetrievalEngineConfig::new(provider_id, model, dimensions, RetrievalAccess::standard());
    // `S = Arc<dyn VectorStore>` / `E = Arc<dyn EmbeddingProvider>` are `Sized`
    // and satisfy their trait bounds via the blanket forwarding impls in
    // `axon-vectors`/`axon-embedding`, so the generic engine is constructed from
    // runtime-held trait objects by wrapping each in one more `Arc`.
    let engine = RetrievalEngine::new(Arc::new(store), Arc::new(provider), config);

    let retrieval_request = RetrievalRequest {
        query: request.query,
        collection: request.collection,
        limit: request.limit.max(1),
        source_id: None,
        generation: None,
        namespace_filters: Vec::new(),
        byte_budget: DEFAULT_BYTE_BUDGET,
        token_budget: DEFAULT_TOKEN_BUDGET,
    };

    let result = engine.retrieve(retrieval_request).await?;

    let hits = result
        .matches
        .into_iter()
        .map(|item| QueryServiceHit {
            canonical_uri: item.canonical_uri,
            chunk_id: item.chunk_id.0,
            score: item.score,
            text: item.text,
        })
        .collect();

    Ok(QueryServiceResult { hits })
}

/// Resolve the embedding provider's authoritative identity from `capabilities()`,
/// falling back to the caller-supplied hints when the capability lacks an
/// embedding block.
async fn resolve_identity(
    provider: &dyn EmbeddingProvider,
    hint_provider_id: ProviderId,
    hint_model: impl Into<String>,
    hint_dimensions: u32,
) -> Result<(ProviderId, String, u32), ApiError> {
    let capability = provider.capabilities().await?;
    let provider_id = capability.provider_id.clone();
    match capability.embedding {
        Some(embedding) => Ok((provider_id, embedding.model_id, embedding.dimensions)),
        None => Ok((provider_id, hint_model.into(), hint_dimensions)),
    }
    .map(|(id, model, dims)| {
        // Keep the hint id only if capabilities somehow returned an empty id.
        if id.0.is_empty() {
            (hint_provider_id.clone(), model, dims)
        } else {
            (id, model, dims)
        }
    })
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
