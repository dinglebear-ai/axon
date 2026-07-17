use async_trait::async_trait;
use axon_api::source::*;

use super::QdrantVectorStore;
use super::capability_snapshot;
use super::commit::{mark_generation_committed_rest, mark_unchanged_items_committed_rest};
use crate::store::{Result, VectorStore};

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()> {
        self.track(self.ensure_collection_inner(spec).await).await
    }

    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult> {
        self.track(self.upsert_inner(batch).await).await
    }

    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult> {
        let http = self.http()?;
        self.track(
            mark_generation_committed_rest(self, &http, collection, source_id, generation).await,
        )
        .await
    }

    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult> {
        let http = self.http()?;
        self.track(
            mark_unchanged_items_committed_rest(
                self,
                &http,
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await,
        )
        .await
    }

    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult> {
        self.track(self.delete_inner(selector).await).await
    }

    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult> {
        self.track(self.search_inner(request).await).await
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(capability_snapshot(self).await)
    }
}
