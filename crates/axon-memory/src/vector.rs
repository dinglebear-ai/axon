//! Vector-backed memory recall boundary.
//!
//! SQLite remains the metadata source of truth. This wrapper adds the contract
//! memory vector namespace on top of any [`MemoryStore`], preserving vector
//! result order by batch-loading SQLite records with `load_many`.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_graph::store::GraphStore;
use axon_vectors::store::VectorStore;
use serde_json::json;
use uuid::Uuid;

use crate::store::{MemoryStore, Result};

mod batch;
mod document;
mod payload;
mod search;
use document::{build_memory_vector_batch, embedding_inputs, prepare_memory_document};
use payload::memory_collection_spec;

pub const MEMORY_VECTOR_NAMESPACE: &str = "memory";
pub const MEMORY_COLLECTION_ALIAS: &str = "memory";

#[derive(Clone)]
pub struct MemoryVectorConfig {
    pub collection: String,
    pub embedding_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub batch_limits: MemoryBatchLimits,
}

/// Bounded batch sizes for bulk memory operations (currently: import). Keeps
/// a single embed/upsert call bounded regardless of how many records a
/// caller hands to `import` at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBatchLimits {
    /// Max records embedded in one `EmbeddingBatch` call.
    pub embed_batch_size: usize,
    /// Max points upserted in one `VectorPointBatch` call. Kept equal to
    /// `embed_batch_size` today (one embed call feeds one upsert call).
    pub upsert_batch_size: usize,
    /// Reserved for a future vector-maintenance scroll. Public memory export
    /// pages authoritative SQLite records and must not reconstruct records
    /// from derived Qdrant points.
    pub qdrant_page_size: usize,
    /// Max memory nodes in one graph mirror transaction.
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

    async fn ensure_collection(&self) -> Result<()> {
        self.vectors
            .ensure_collection(memory_collection_spec(&self.config))
            .await
    }

    async fn upsert_record(&self, record: &MemoryRecord) -> Result<Vec<VectorPointId>> {
        self.ensure_collection().await?;
        let document = prepare_memory_document(record)?;
        let batch_id = BatchId::new(Uuid::new_v4());
        let job_id = JobId::new(Uuid::new_v4());
        let embedding = self
            .embeddings
            .embed(EmbeddingBatch {
                batch_id,
                job_id,
                provider_id: self.config.embedding_provider_id.clone(),
                model: self.config.embedding_model.clone(),
                items: embedding_inputs(&document),
                instruction: None,
                priority: JobPriority::Normal,
                metadata: document.metadata.clone(),
            })
            .await?;
        let batch = build_memory_vector_batch(&self.config, document, embedding)?;
        let point_ids = batch
            .points
            .iter()
            .map(|point| point.point_id.clone())
            .collect::<Vec<_>>();
        self.vectors.upsert(batch).await?;
        Ok(point_ids)
    }

    async fn hide_vectors(&self, memory_id: &MemoryId) -> Result<()> {
        self.vectors
            .delete(VectorDeleteSelector::Filter {
                collection: self.config.collection.clone(),
                filter: json!({
                    "vector_namespace": MEMORY_VECTOR_NAMESPACE,
                    "memory_id": memory_id.0,
                }),
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for VectorBackedMemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult> {
        let should_embed = request.embed;
        let mut result = self.inner.remember(request).await?;
        if should_embed {
            let record = self
                .inner
                .get(result.memory_id.clone())
                .await?
                .ok_or_else(|| missing_memory(&result.memory_id))?;
            result.vector_point_ids = self.upsert_record(&record).await?;
        }
        Ok(result)
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
        let result = self.inner.supersede(request.clone()).await?;
        self.hide_vectors(&request.memory_id).await?;
        Ok(result)
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        self.inner.contradict(request).await
    }

    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        let result = self.inner.set_status(request.clone()).await?;
        if matches!(
            request.status,
            MemoryStatus::Forgotten | MemoryStatus::Archived | MemoryStatus::Superseded
        ) {
            self.hide_vectors(&request.memory_id).await?;
        }
        Ok(result)
    }

    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        self.inner.review(request).await
    }

    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        let body_changed = request.body.is_some();
        let memory_id = request.memory_id.clone();
        let result = self.inner.update(request).await?;
        if body_changed {
            let record = self
                .inner
                .get(memory_id)
                .await?
                .ok_or_else(|| missing_memory(&result.memory_id))?;
            self.upsert_record(&record).await?;
        }
        Ok(result)
    }

    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        self.inner.pin(request).await
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let result = self.inner.archive(request).await?;
        self.hide_vectors(&memory_id).await?;
        Ok(result)
    }

    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let result = self.inner.forget(request).await?;
        self.hide_vectors(&memory_id).await?;
        Ok(result)
    }

    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        let archive_sources = request.archive_sources;
        let source_ids = request.memory_ids.clone();
        let result = self.inner.compact(request).await?;
        let record = self
            .inner
            .get(result.memory_id.clone())
            .await?
            .ok_or_else(|| missing_memory(&result.memory_id))?;
        self.upsert_record(&record).await?;
        if archive_sources {
            for source_id in source_ids {
                self.hide_vectors(&source_id).await?;
            }
        }
        Ok(result)
    }

    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        let dry_run = request.dry_run;
        let mut result = self.inner.import(request).await?;
        if dry_run || result.created_ids.is_empty() {
            return Ok(result);
        }

        let mut records = Vec::with_capacity(result.created_ids.len());
        for memory_id in &result.created_ids {
            if let Some(record) = self.inner.get(memory_id.clone()).await? {
                records.push(record);
            }
        }

        let outcomes = self.upsert_records_batched(&records).await?;
        for (memory_id, outcome) in outcomes {
            if let Err(error) = outcome {
                // Partial vector failure: the SQLite row is durable, but it
                // must not silently claim to be recallable when it has no
                // vector — send it to review with a recovery marker instead
                // of failing the whole import.
                self.inner
                    .set_status(MemoryStatusRequest {
                        memory_id: memory_id.clone(),
                        status: MemoryStatus::Review,
                        reason: Some(format!("memory.vector_failed: {}", error.message)),
                        timestamp: Timestamp::from(chrono::Utc::now()),
                    })
                    .await?;
                result.warnings.push(SourceWarning {
                    code: "memory.vector_failed".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "memory {} imported but embedding failed; sent to review",
                        memory_id.0
                    ),
                    source_item_key: None,
                    retryable: true,
                });
            }
        }
        Ok(result)
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
            "vector_namespace".to_string(),
            json!(MEMORY_VECTOR_NAMESPACE),
        );
        Ok(capability)
    }
}

fn missing_memory(memory_id: &MemoryId) -> ApiError {
    ApiError::new(
        "memory.not_found",
        axon_error::ErrorStage::Retrieving,
        format!("memory {} not found after metadata write", memory_id.0),
    )
}
#[cfg(test)]
#[path = "vector_tests.rs"]
mod tests;
