//! Blanket `VectorStore` forwarding impl for boxed trait objects.
//!
//! Lets a runtime-held `Arc<dyn VectorStore>` itself satisfy a `S: VectorStore`
//! bound, so generic consumers such as the retrieval engine can be constructed
//! from a trait object without monomorphizing over the concrete store type.
//! Kept in its own module so `store.rs` stays within the monolith line budget.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;

use super::{Result, VectorStore};

#[async_trait]
impl VectorStore for Arc<dyn VectorStore> {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()> {
        (**self).ensure_collection(spec).await
    }
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult> {
        (**self).upsert(batch).await
    }
    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult> {
        (**self)
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
    ) -> Result<VectorStoreWriteResult> {
        (**self)
            .mark_unchanged_items_committed(
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await
    }
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult> {
        (**self).delete(selector).await
    }
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult> {
        (**self).search(request).await
    }
    async fn capabilities(&self) -> Result<ProviderCapability> {
        (**self).capabilities().await
    }
}
