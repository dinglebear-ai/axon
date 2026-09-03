//! Vector-lane reservation helpers.

use std::future::Future;
use std::sync::Arc;

use axon_api::source::{
    ApiError, ErrorStage, SourceGenerationId, SourceId, SourceItemKey, VectorDeleteSelector,
    VectorStoreDeleteResult, VectorStoreWriteResult,
};
use axon_jobs::scheduler::call_reserved;

use crate::context::TargetLocalSourceRuntime;

use super::{ProviderCallContext, VectorLane, map_reserved};

pub async fn begin_bulk_load(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
) -> Result<(), ApiError> {
    bulk_load_operation(runtime, context, collection, false).await
}

pub async fn finish_bulk_load(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
) -> Result<(), ApiError> {
    bulk_load_operation(runtime, context, collection, true).await
}

pub async fn with_bulk_load<F>(
    runtime: &TargetLocalSourceRuntime,
    begin_context: ProviderCallContext,
    finish_context: ProviderCallContext,
    collection: String,
    failure_context: &str,
    processing: F,
) -> anyhow::Result<()>
where
    F: Future<Output = anyhow::Result<()>>,
{
    begin_bulk_load(runtime, begin_context, collection.clone()).await?;
    let mut guard =
        BulkLoadCompletionGuard::new(Arc::clone(&runtime.vector_store), collection.clone());
    let processing = processing.await;
    let finishing = finish_bulk_load(runtime, finish_context, collection).await;
    guard.disarm();
    match (processing, finishing) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(finish_error)) => {
            Err(error.context(format!("{failure_context}: {finish_error}")))
        }
    }
}

struct BulkLoadCompletionGuard {
    store: Arc<dyn axon_vectors::store::VectorStore>,
    collection: String,
    armed: bool,
}

impl BulkLoadCompletionGuard {
    fn new(store: Arc<dyn axon_vectors::store::VectorStore>, collection: String) -> Self {
        Self {
            store,
            collection,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for BulkLoadCompletionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let store = Arc::clone(&self.store);
        let collection = self.collection.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = store.finish_bulk_load(&collection).await {
                    tracing::error!(%error, %collection, "failed to restore bulk-load state after pipeline cancellation");
                }
            });
        }
    }
}

#[cfg(test)]
pub(crate) fn test_bulk_load_completion_guard(
    store: Arc<dyn axon_vectors::store::VectorStore>,
    collection: String,
) -> impl Drop {
    BulkLoadCompletionGuard::new(store, collection)
}

async fn bulk_load_operation(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    finish: bool,
) -> Result<(), ApiError> {
    let store = Arc::clone(&runtime.vector_store);
    vector_operation(runtime, context, move || async move {
        if finish {
            store.finish_bulk_load(&collection).await
        } else {
            store.begin_bulk_load(&collection).await
        }
    })
    .await
}

/// Run an arbitrary vector-capacity operation under the durable scheduler.
/// This is used for Qdrant-specific read helpers whose API intentionally sits
/// outside the generic `VectorStore` trait.
pub async fn vector_operation<T, F, Fut>(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return operation().await;
    };
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| operation(),
        )
        .await,
        ErrorStage::Retrieving,
        "vector",
    )
}

pub async fn mark_generation_committed(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .mark_generation_committed(collection, source_id, generation)
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .mark_generation_committed(collection, source_id, generation)
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}

pub async fn mark_unchanged_items_committed(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    previous_generation: SourceGenerationId,
    committed_generation: SourceGenerationId,
    source_item_keys: Vec<SourceItemKey>,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .mark_unchanged_items_committed(
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .mark_unchanged_items_committed(
                        collection,
                        source_id,
                        previous_generation,
                        committed_generation,
                        source_item_keys,
                    )
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}

pub async fn delete_vectors(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    selector: VectorDeleteSelector,
) -> Result<VectorStoreDeleteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime.vector_store.delete(selector).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.delete(selector).await },
        )
        .await,
        ErrorStage::Cleaning,
        "vector",
    )
}

pub async fn retire_generation(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
    retired_epoch: SourceGenerationId,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .retire_generation(collection, source_id, generation, retired_epoch)
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .retire_generation(collection, source_id, generation, retired_epoch)
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}
