use super::*;
use async_trait::async_trait;
use axon_embedding::{fake::FakeEmbeddingProvider, provider::EmbeddingProvider};
use axon_vectors::store::{FakeVectorStore, VectorStore};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, Semaphore, mpsc};

pub(super) struct ObservedEmbedding {
    pub inner: FakeEmbeddingProvider,
    pub started: mpsc::UnboundedSender<usize>,
    pub first_gate: Option<Arc<Semaphore>>,
    pub fail_at: Option<usize>,
    pub upsert_active: Arc<AtomicBool>,
    pub overlapped: AtomicBool,
}

#[async_trait]
impl EmbeddingProvider for ObservedEmbedding {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        let index = batch.items[0]
            .chunk_id
            .0
            .strip_prefix("chunk-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        if self.upsert_active.load(Ordering::Acquire) {
            self.overlapped.store(true, Ordering::Release);
        }
        self.started.send(index).unwrap();
        if index == 0 {
            if let Some(gate) = &self.first_gate {
                gate.acquire().await.unwrap().forget();
            }
        }
        if self.fail_at == Some(index) {
            return Err(ApiError::new(
                "embedding.test.failure",
                ErrorStage::Embedding,
                "scheduler test embedding failure",
            ));
        }
        self.inner.embed(batch).await
    }
    async fn capabilities(&self) -> axon_embedding::provider::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}

pub(super) struct ObservedVectors {
    pub inner: FakeVectorStore,
    pub started: mpsc::UnboundedSender<usize>,
    pub first_gate: Option<Arc<Semaphore>>,
    pub active: Arc<AtomicBool>,
    pub order: Mutex<Vec<usize>>,
}

#[async_trait]
impl VectorStore for ObservedVectors {
    async fn ensure_collection(&self, spec: CollectionSpec) -> axon_vectors::store::Result<()> {
        self.inner.ensure_collection(spec).await
    }
    async fn upsert(
        &self,
        batch: VectorPointBatch,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        let index = batch.points[0]
            .chunk_id
            .0
            .strip_prefix("chunk-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        self.active.store(true, Ordering::Release);
        self.started.send(index).unwrap();
        if index == 0 {
            if let Some(gate) = &self.first_gate {
                gate.acquire().await.unwrap().forget();
            }
        }
        let result = self.inner.upsert(batch).await;
        self.order.lock().await.push(index);
        self.active.store(false, Ordering::Release);
        result
    }
    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .mark_generation_committed(collection, source_id, generation)
            .await
    }

    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .mark_unchanged_items_committed(
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await
    }

    async fn retire_generation(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
        retired_epoch: SourceGenerationId,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .retire_generation(collection, source_id, generation, retired_epoch)
            .await
    }

    async fn delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> axon_vectors::store::Result<VectorStoreDeleteResult> {
        self.inner.delete(selector).await
    }

    async fn search(
        &self,
        request: VectorSearchRequest,
    ) -> axon_vectors::store::Result<VectorSearchResult> {
        self.inner.search(request).await
    }

    async fn capabilities(&self) -> axon_vectors::store::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}
