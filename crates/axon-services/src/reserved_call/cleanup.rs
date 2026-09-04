//! Scheduler-backed provider operations for autonomous cleanup debt.

use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::SourceAdapterRegistry;
use axon_api::source::*;
use axon_graph::store::GraphStore;
use axon_jobs::scheduler::{ReservedCallError, call_reserved};

use crate::context::{QueueSummaryTask, ServiceContext, TargetLocalSourceRuntime};
use crate::source::prune::{CleanupProviderOps, DebtDrainSummary};

use super::{ArtifactLane, GraphLane, ProviderCallContext, map_reserved};

struct ScheduledCleanupProviderOps {
    runtime: Arc<TargetLocalSourceRuntime>,
    graph_store: Option<Arc<dyn GraphStore>>,
    job_id: JobId,
}

impl ScheduledCleanupProviderOps {
    fn context(&self, operation: &str) -> ProviderCallContext {
        cleanup_context(self.job_id, operation)
    }
}

fn cleanup_context(job_id: JobId, operation: &str) -> ProviderCallContext {
    ProviderCallContext::for_phase(
        job_id,
        0,
        PipelinePhase::Cleaning,
        JobPriority::Background,
        format!("cleanup-debt:{operation}:{}", uuid::Uuid::new_v4()),
    )
}

#[async_trait]
impl CleanupProviderOps for ScheduledCleanupProviderOps {
    async fn vector_delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult, ApiError> {
        super::delete_vectors(
            self.runtime.as_ref(),
            self.context("vector-delete"),
            selector,
        )
        .await
    }

    async fn graph_delete_nodes(
        &self,
        stable_keys: Vec<String>,
    ) -> Result<GraphDeleteResult, ApiError> {
        let store = self.graph_store.clone().ok_or_else(missing_graph_store)?;
        graph_cleanup_call(
            self.runtime.as_ref(),
            self.context("graph-delete-nodes"),
            move || async move { store.delete_nodes(stable_keys).await },
        )
        .await
    }

    async fn graph_delete_edges(
        &self,
        edge_ids: Vec<GraphEdgeId>,
    ) -> Result<GraphDeleteResult, ApiError> {
        let store = self.graph_store.clone().ok_or_else(missing_graph_store)?;
        graph_cleanup_call(
            self.runtime.as_ref(),
            self.context("graph-delete-edges"),
            move || async move { store.delete_edges(edge_ids).await },
        )
        .await
    }

    async fn artifact_delete(&self, handle: ArtifactHandle) -> Result<(), ApiError> {
        let Some(scheduler) = self.runtime.artifact_scheduler.as_deref() else {
            return self.runtime.artifact_store.delete(handle).await;
        };
        let store = Arc::clone(&self.runtime.artifact_store);
        map_reserved(
            call_reserved::<ArtifactLane, _, ApiError, _, _>(
                scheduler,
                self.context("artifact-delete").request(1),
                move |_lease| async move { store.delete(handle).await },
            )
            .await,
            ErrorStage::Cleaning,
            "artifact",
        )
    }
}

async fn graph_cleanup_call<T, F, Fut>(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let Some(scheduler) = runtime.graph_scheduler.as_deref() else {
        return operation().await;
    };
    match call_reserved::<GraphLane, _, ApiError, _, _>(
        scheduler,
        context.request(1),
        move |_lease| operation(),
    )
    .await
    {
        Ok(value) => Ok(value),
        Err(ReservedCallError::Provider(error)) => Err(error),
        Err(ReservedCallError::Scheduler(error)) => {
            Err(super::scheduler_error(error, ErrorStage::Cleaning, "graph"))
        }
    }
}

fn missing_graph_store() -> ApiError {
    ApiError::new(
        "source.cleanup.provider_unavailable",
        ErrorStage::Cleaning,
        "no GraphStore wired for cleanup debt",
    )
}

pub async fn drain_source_cleanup_debt(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    collection: &str,
    counts: &crate::source::result_map::IndexCounts,
) -> DebtDrainSummary {
    let (graph_store, memory_store) = crate::source::open_cleanup_debt_stores(ctx).await;
    let registry = runtime.source_adapter_registry(ctx).await.ok();
    let providers = ScheduledCleanupProviderOps {
        runtime: Arc::new(runtime.clone()),
        graph_store,
        job_id: counts.job_id,
    };
    crate::source::prune::drain_cleanup_debt_with_provider_ops(
        runtime.ledger.as_ref(),
        &providers,
        memory_store.as_deref(),
        Some(runtime.jobs.as_ref()),
        Some(runtime.document_cache.as_ref()),
        registry,
        collection,
        counts,
    )
    .await
}

pub async fn spawn_cleanup_debt_worker(
    ctx: &ServiceContext,
    source_runtime: &TargetLocalSourceRuntime,
    registry: SourceAdapterRegistry,
) -> std::io::Result<Arc<QueueSummaryTask>> {
    let source_runtime = Arc::new(source_runtime.clone());
    let ledger = Arc::clone(&source_runtime.ledger);
    let job_store = Arc::clone(&source_runtime.jobs);
    let document_cache = Arc::clone(&source_runtime.document_cache);
    let collection = ctx.cfg.collection.clone();
    let (graph_store, memory_store) = crate::source::open_cleanup_debt_stores(ctx).await;
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (stop, stopped) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("axon-cleanup-debt".into())
        .spawn(move || {
            loop {
                if stopped
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .is_ok()
                {
                    break;
                }
                tokio_runtime.block_on(run_sweep(
                    &source_runtime,
                    graph_store.as_ref(),
                    memory_store.as_ref(),
                    &registry,
                    &collection,
                    ledger.as_ref(),
                    job_store.as_ref(),
                    document_cache.as_ref(),
                ));
            }
        })?;
    Ok(Arc::new(QueueSummaryTask::new(stop, thread)))
}

#[allow(clippy::too_many_arguments)]
async fn run_sweep(
    runtime: &Arc<TargetLocalSourceRuntime>,
    graph_store: Option<&Arc<dyn GraphStore>>,
    memory_store: Option<&Arc<dyn axon_memory::store::MemoryStore>>,
    registry: &SourceAdapterRegistry,
    collection: &str,
    ledger: &dyn axon_ledger::store::LedgerStore,
    job_store: &dyn axon_jobs::boundary::JobStore,
    document_cache: &dyn axon_core::boundary::DocumentCache,
) -> DebtDrainSummary {
    crate::source::prune::drain_all_cleanup_debt(ledger, 256, |counts| {
        let providers = ScheduledCleanupProviderOps {
            runtime: Arc::clone(runtime),
            graph_store: graph_store.cloned(),
            job_id: counts.job_id,
        };
        async move {
            crate::source::prune::drain_cleanup_debt_with_provider_ops(
                ledger,
                &providers,
                memory_store.map(AsRef::as_ref),
                Some(job_store),
                Some(document_cache),
                Some(registry),
                collection,
                &counts,
            )
            .await
        }
    })
    .await
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
