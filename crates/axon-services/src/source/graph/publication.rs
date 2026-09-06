//! Bounded graph-publication scheduling and result aggregation.

use std::sync::Arc;

use axon_api::source::{ApiError, ErrorStage, GraphCandidate, GraphWriteResult};
use sqlx::SqlitePool;

use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ProviderCallContext};

/// Drain the generation lazily. A committed prefix may be visible if a later
/// batch fails; the caller reports degradation and retries the idempotent work.
pub(super) async fn upsert_candidate_batches<I>(
    runtime: Option<&TargetLocalSourceRuntime>,
    context: Option<ProviderCallContext>,
    pool: &Arc<SqlitePool>,
    candidates: I,
    batch_size: usize,
) -> Result<GraphWriteResult, ApiError>
where
    I: IntoIterator<Item = GraphCandidate>,
{
    let mut total = None;
    let mut batch = Vec::with_capacity(batch_size);
    for candidate in candidates {
        batch.push(candidate);
        if batch.len() == batch_size {
            merge(
                &mut total,
                upsert(runtime, context.clone(), pool, batch).await?,
            );
            batch = Vec::with_capacity(batch_size);
        }
    }
    if !batch.is_empty() {
        merge(&mut total, upsert(runtime, context, pool, batch).await?);
    }
    total.ok_or_else(|| {
        ApiError::new(
            "graph.empty_publication",
            ErrorStage::Graphing,
            "graph publication produced no candidate batches",
        )
    })
}

async fn upsert(
    runtime: Option<&TargetLocalSourceRuntime>,
    context: Option<ProviderCallContext>,
    pool: &Arc<SqlitePool>,
    candidates: Vec<GraphCandidate>,
) -> Result<GraphWriteResult, ApiError> {
    match (runtime, context) {
        (Some(runtime), Some(context)) => {
            reserved_call::upsert_graph_candidates(
                runtime,
                context,
                pool.as_ref().clone(),
                candidates,
            )
            .await
        }
        #[cfg(test)]
        (None, None) => {
            reserved_call::upsert_graph_candidates_for_test(pool.as_ref().clone(), candidates).await
        }
        _ => Err(ApiError::new(
            "graph.runtime_missing",
            ErrorStage::Graphing,
            "graph write is missing scheduler runtime/context",
        )),
    }
}

fn merge(total: &mut Option<GraphWriteResult>, result: GraphWriteResult) {
    if let Some(total) = total {
        total.candidates_seen = total.candidates_seen.saturating_add(result.candidates_seen);
        total.nodes_upserted = total.nodes_upserted.saturating_add(result.nodes_upserted);
        total.edges_upserted = total.edges_upserted.saturating_add(result.edges_upserted);
        total.evidence_records = total
            .evidence_records
            .saturating_add(result.evidence_records);
        total.warnings.extend(result.warnings);
    } else {
        *total = Some(result);
    }
}
