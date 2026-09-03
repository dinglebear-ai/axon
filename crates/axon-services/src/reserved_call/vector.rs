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

#[derive(Default)]
struct CleanupRegistry {
    handles: Vec<std::thread::JoinHandle<()>>,
    draining: bool,
}

static BULK_LOAD_CLEANUPS: std::sync::LazyLock<Arc<std::sync::Mutex<CleanupRegistry>>> =
    std::sync::LazyLock::new(|| Arc::new(std::sync::Mutex::new(CleanupRegistry::default())));

pub fn drain_bulk_load_cleanups() {
    // Mark the lower-level transition registry as draining first. Any
    // transition started by a service cleanup below is then joined inline.
    axon_vectors::qdrant::drain_bulk_load_transition_workers();
    drain_bulk_load_cleanups_in(&BULK_LOAD_CLEANUPS);
}

fn drain_bulk_load_cleanups_in(cleanups: &std::sync::Mutex<CleanupRegistry>) {
    let cleanups = {
        let mut pending = cleanups
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.draining = true;
        std::mem::take(&mut pending.handles)
    };
    for cleanup in cleanups {
        if cleanup.join().is_err() {
            tracing::error!("bulk-load cancellation cleanup thread panicked");
        }
    }
}

fn track_bulk_load_cleanup_in(
    cleanups: &std::sync::Mutex<CleanupRegistry>,
    cleanup: std::thread::JoinHandle<()>,
) {
    let (finished, late_cleanup) = {
        let mut pending = cleanups
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut finished = Vec::new();
        let mut index = pending.handles.len();
        while index > 0 {
            index -= 1;
            if pending.handles[index].is_finished() {
                finished.push(pending.handles.swap_remove(index));
            }
        }
        if pending.draining {
            (finished, Some(cleanup))
        } else {
            pending.handles.push(cleanup);
            (finished, None)
        }
    };
    for cleanup in finished.into_iter().chain(late_cleanup) {
        if cleanup.join().is_err() {
            tracing::error!("bulk-load cancellation cleanup thread panicked");
        }
    }
}

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
    finish_bulk_load_with_handoff(runtime, context, collection, || {}).await
}

async fn finish_bulk_load_with_handoff(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    handoff: impl FnOnce(),
) -> Result<(), ApiError> {
    let store = Arc::clone(&runtime.vector_store);
    vector_operation(runtime, context, move || {
        let completion = start_finish_then_handoff(store, collection, handoff);
        async move {
            completion.await.map_err(|_| {
                ApiError::new(
                    "vector.bulk_load_finish_worker_lost",
                    ErrorStage::Publishing,
                    "bulk-load finish worker ended without a result",
                )
            })?
        }
    })
    .await
}

fn start_finish_then_handoff(
    store: Arc<dyn axon_vectors::store::VectorStore>,
    collection: String,
    handoff: impl FnOnce(),
) -> tokio::sync::oneshot::Receiver<Result<(), ApiError>> {
    let completion = start_bulk_load_finish(store, collection);
    handoff();
    completion
}

fn start_bulk_load_finish(
    store: Arc<dyn axon_vectors::store::VectorStore>,
    collection: String,
) -> tokio::sync::oneshot::Receiver<Result<(), ApiError>> {
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let cleanup = std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build bulk-load finish runtime");
            runtime.block_on(store.finish_bulk_load(&collection))
        }));
        if let Ok(result) = result {
            let _ = result_tx.send(result);
        }
    });
    track_bulk_load_cleanup_in(&BULK_LOAD_CLEANUPS, cleanup);
    result_rx
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
    let finishing = finish_bulk_load_with_handoff(runtime, finish_context, collection, || {
        guard.disarm();
    })
    .await;
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
    cleanups: Arc<std::sync::Mutex<CleanupRegistry>>,
    armed: bool,
}

impl BulkLoadCompletionGuard {
    fn new(store: Arc<dyn axon_vectors::store::VectorStore>, collection: String) -> Self {
        Self {
            store,
            collection,
            cleanups: Arc::clone(&BULK_LOAD_CLEANUPS),
            armed: true,
        }
    }

    #[cfg(test)]
    fn with_registry(
        store: Arc<dyn axon_vectors::store::VectorStore>,
        collection: String,
        cleanups: Arc<std::sync::Mutex<CleanupRegistry>>,
    ) -> Self {
        Self {
            store,
            collection,
            cleanups,
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
        let cleanups = Arc::clone(&self.cleanups);
        let failure_collection = collection.clone();
        let cleanup = std::thread::spawn(move || {
            let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build bulk-load cancellation cleanup runtime");
                runtime.block_on(async move {
                    if let Err(error) = store.finish_bulk_load(&collection).await {
                        tracing::error!(%error, %collection, "failed to restore bulk-load state after pipeline cancellation");
                    }
                });
            }));
            if cleanup.is_err() {
                tracing::error!(collection = %failure_collection, "bulk-load cancellation cleanup thread panicked");
            }
        });
        track_bulk_load_cleanup_in(&cleanups, cleanup);
    }
}

#[cfg(test)]
pub(crate) fn test_bulk_load_cleanup_lifecycle(
    store: Arc<dyn axon_vectors::store::VectorStore>,
    collection: String,
) {
    let cleanups = Arc::new(std::sync::Mutex::new(CleanupRegistry::default()));
    drop(BulkLoadCompletionGuard::with_registry(
        Arc::clone(&store),
        collection.clone(),
        Arc::clone(&cleanups),
    ));
    drain_bulk_load_cleanups_in(&cleanups);
    drop(BulkLoadCompletionGuard::with_registry(
        store, collection, cleanups,
    ));
}

#[cfg(test)]
pub(crate) async fn test_bulk_load_finish_handoff(
    store: Arc<axon_vectors::store::FakeVectorStore>,
    collection: String,
) {
    let observer = Arc::clone(&store);
    let (handoff_tx, handoff_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let completion = start_finish_then_handoff(store, collection, || {
            let _ = handoff_tx.send(());
        });
        let _ = completion.await;
    });
    handoff_rx.await.expect("finish worker must take ownership");
    task.abort();
    let _ = task.await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while observer.calls().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("owned finish worker must complete after caller cancellation");
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
