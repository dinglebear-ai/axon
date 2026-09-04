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

struct CleanupDrainContext<'a> {
    runtime: Arc<TargetLocalSourceRuntime>,
    graph_store: Option<Arc<dyn GraphStore>>,
    memory_store: Option<&'a dyn axon_memory::store::MemoryStore>,
    registry: Option<&'a SourceAdapterRegistry>,
    collection: &'a str,
}

// LEARNED: publication and autonomous cleanup independently wiring the same
// provider set creates two policy surfaces that can silently drift.
// PATTERN: one drain context owns provider construction and all shared debt
// dependencies; each caller supplies only the debt counts it is draining.
async fn drain_with_context(
    context: &CleanupDrainContext<'_>,
    counts: &crate::source::result_map::IndexCounts,
) -> DebtDrainSummary {
    let providers = ScheduledCleanupProviderOps {
        runtime: Arc::clone(&context.runtime),
        graph_store: context.graph_store.clone(),
        job_id: counts.job_id,
    };
    crate::source::prune::drain_cleanup_debt_with_provider_ops(
        context.runtime.ledger.as_ref(),
        &providers,
        context.memory_store,
        Some(context.runtime.jobs.as_ref()),
        Some(context.runtime.document_cache.as_ref()),
        context.registry,
        context.collection,
        counts,
    )
    .await
}

pub async fn drain_source_cleanup_debt(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    collection: &str,
    counts: &crate::source::result_map::IndexCounts,
) -> DebtDrainSummary {
    let (graph_store, memory_store) = crate::source::open_cleanup_debt_stores(ctx).await;
    let registry_result = runtime.source_adapter_registry(ctx).await;
    let registry = registry_result.as_ref().ok().copied();
    let drain = CleanupDrainContext {
        runtime: Arc::new(runtime.clone()),
        graph_store,
        memory_store: memory_store.as_deref(),
        registry,
        collection,
    };
    let mut summary = drain_with_context(&drain, counts).await;
    if let Err(error) = registry_result {
        record_registry_construction_failure(runtime, counts, &error, &mut summary).await;
    }
    summary
}

async fn record_registry_construction_failure(
    runtime: &TargetLocalSourceRuntime,
    counts: &crate::source::result_map::IndexCounts,
    error: &anyhow::Error,
    summary: &mut DebtDrainSummary,
) {
    match runtime
        .ledger
        .list_pending_cleanup_debt(counts.source_id.clone())
        .await
    {
        Ok(debts) => {
            let affected = count_adapter_release_debt(debts.iter().map(|debt| debt.kind));
            mark_registry_failure(summary, affected);
            if affected > 0 {
                tracing::warn!(
                    source_id = %counts.source_id.0,
                    affected,
                    code = "source.cleanup.adapter_registry_unavailable",
                    error = %error,
                    "adapter cleanup debt remains pending because its registry could not be constructed"
                );
            }
        }
        Err(enumeration_error) => {
            summary.failed = summary.failed.saturating_add(1);
            summary.enumeration_failed = true;
            tracing::warn!(
                source_id = %counts.source_id.0,
                registry_error = %error,
                error = %enumeration_error.message,
                "failed to enumerate adapter cleanup debt after registry construction failure"
            );
        }
    }
}

fn count_adapter_release_debt(kinds: impl IntoIterator<Item = CleanupDebtKind>) -> u64 {
    kinds
        .into_iter()
        .filter(|kind| *kind == CleanupDebtKind::AdapterRelease)
        .count() as u64
}

fn mark_registry_failure(summary: &mut DebtDrainSummary, affected: u64) {
    summary.failed = summary.failed.saturating_add(affected);
}

pub async fn spawn_cleanup_debt_worker(
    ctx: &ServiceContext,
    source_runtime: &TargetLocalSourceRuntime,
    registry: SourceAdapterRegistry,
) -> std::io::Result<Arc<QueueSummaryTask>> {
    let source_runtime = Arc::new(source_runtime.clone());
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
                let drain = CleanupDrainContext {
                    runtime: Arc::clone(&source_runtime),
                    graph_store: graph_store.clone(),
                    memory_store: memory_store.as_deref(),
                    registry: Some(&registry),
                    collection: &collection,
                };
                tokio_runtime.block_on(run_sweep(&drain));
            }
        })?;
    Ok(Arc::new(QueueSummaryTask::new(stop, thread)))
}

async fn run_sweep(context: &CleanupDrainContext<'_>) -> DebtDrainSummary {
    crate::source::prune::drain_all_cleanup_debt(
        context.runtime.ledger.as_ref(),
        256,
        |counts| async move { drain_with_context(context, &counts).await },
    )
    .await
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
