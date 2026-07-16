//! Vector-backed memory recall boundary.
//!
//! SQLite remains the metadata source of truth. Mutations are deliberately
//! delegated unchanged: `axon-services` republishes affected `memory://`
//! identities through the canonical source pipeline. This wrapper only adds
//! semantic recall over those canonical vectors.

use std::sync::Arc;

use crate::store::{MemoryStore, Result};
use async_trait::async_trait;
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_graph::store::GraphStore;
use axon_vectors::store::VectorStore;

mod search;

#[derive(Clone)]
pub struct MemoryVectorConfig {
    pub collection: String,
    pub embedding_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub batch_limits: MemoryBatchLimits,
}

/// Retained memory processing limits shared with graph/import orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBatchLimits {
    /// Max records considered in one canonical source-sync batch.
    pub embed_batch_size: usize,
    /// Max records published in one vector-side batch.
    pub upsert_batch_size: usize,
    /// Reserved for a future vector-maintenance scroll. Public memory export
    /// pages authoritative SQLite records and must not reconstruct records
    /// from derived Qdrant points.
    pub qdrant_page_size: usize,
    /// Max memory nodes in one graph transaction.
    pub graph_tx_batch_size: usize,
}

impl Default for MemoryBatchLimits {
    fn default() -> Self {
        Self {
            embed_batch_size: 32,
            upsert_batch_size: 32,
            qdrant_page_size: 256,
            graph_tx_batch_size: 50,
        }
    }
}

pub struct VectorBackedMemoryStore {
    inner: Arc<dyn MemoryStore>,
    embeddings: Arc<dyn EmbeddingProvider>,
    vectors: Arc<dyn VectorStore>,
    graph: Option<Arc<dyn GraphStore>>,
    config: MemoryVectorConfig,
}

impl VectorBackedMemoryStore {
    pub fn new(
        inner: Arc<dyn MemoryStore>,
        embeddings: Arc<dyn EmbeddingProvider>,
        vectors: Arc<dyn VectorStore>,
        config: MemoryVectorConfig,
    ) -> Self {
        Self {
            inner,
            embeddings,
            vectors,
            graph: None,
            config,
        }
    }

    pub fn with_graph_store(mut self, graph: Arc<dyn GraphStore>) -> Self {
        self.graph = Some(graph);
        self
    }
}

#[async_trait]
impl MemoryStore for VectorBackedMemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult> {
        self.inner.remember(request).await
    }

    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>> {
        self.inner.get(memory_id).await
    }

    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        self.inner.load_many(memory_ids).await
    }

    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult> {
        if request.query.trim().is_empty() {
            return self.inner.search(request).await;
        }
        self.search_vectors(&request).await
    }

    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult> {
        self.inner.context(request).await
    }

    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult> {
        self.inner.link(request).await
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult> {
        self.inner.reinforce(memory_id, signal).await
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        self.inner.supersede(request).await
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        self.inner.contradict(request).await
    }

    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        self.inner.set_status(request).await
    }

    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        self.inner.review(request).await
    }

    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        self.inner.update(request).await
    }

    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        self.inner.pin(request).await
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        self.inner.archive(request).await
    }

    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        self.inner.forget(request).await
    }

    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        self.inner.compact(request).await
    }

    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        self.inner.import(request).await
    }

    async fn export(&self, request: MemoryExportRequest) -> Result<MemoryExportResult> {
        self.inner.export(request).await
    }

    async fn reset(&self) -> Result<()> {
        self.inner.reset().await
    }

    async fn capabilities(&self) -> Result<MemoryStoreCapability> {
        let mut capability = self.inner.capabilities().await?;
        capability.0.features.push("vector_recall".to_string());
        capability.0.limits.insert(
            "recall_payload_filter".to_string(),
            serde_json::json!({"source_kind": "memory"}),
        );
        Ok(capability)
    }
}
#[cfg(test)]
#[path = "vector_tests.rs"]
mod tests;
