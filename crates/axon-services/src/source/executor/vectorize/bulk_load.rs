//! Deferred bulk lifecycle helpers shared by both vectorization paths.
use super::*;

pub(super) fn bulk_context(
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    action: &str,
) -> ProviderCallContext {
    ProviderCallContext::for_phase(
        input.plan.job_id,
        input.execution.attempt,
        PipelinePhase::Upserting,
        input.execution.priority,
        format!("{action}:{}", collection.collection),
    )
}

pub(super) async fn finish_bulk_result<T>(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    guard: Option<reserved_call::BulkLoadCompletionGuard>,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    let cleanup = match guard {
        Some(guard) => {
            guard
                .finish(runtime, bulk_context(input, collection, "finish-bulk-load"))
                .await
        }
        None => Ok(()),
    };
    match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("restoring vector indexing also failed: {cleanup}")))
        }
    }
}

/// Lazy provider lifecycle shared by every unscheduled acquisition batch.
#[derive(Default)]
pub(in crate::source::executor) struct GenerationVectorState {
    collection: Option<CollectionSpec>,
    guard: Option<reserved_call::BulkLoadCompletionGuard>,
}

impl GenerationVectorState {
    pub(super) async fn ensure(
        &mut self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
    ) -> anyhow::Result<CollectionSpec> {
        if let Some(collection) = &self.collection {
            return Ok(collection.clone());
        }
        super::super::helpers::ensure_providers_ready(runtime).await?;
        let plane = runtime.verified_embedding_plane().await?;
        let collection =
            super::super::helpers::collection_spec(input.collection, plane.identity.dimensions);
        super::super::created_generation::setup::ensure_generation_collection(
            runtime,
            input,
            &collection,
        )
        .await?;
        self.guard = Some(
            reserved_call::start_bulk_load_guard(
                runtime,
                bulk_context(input, &collection, "begin-bulk-load"),
                collection.collection.clone(),
            )
            .await?,
        );
        self.collection = Some(collection.clone());
        Ok(collection)
    }

    pub(in crate::source::executor) async fn finish<T>(
        self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
        result: anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        match self.collection {
            Some(collection) => {
                finish_bulk_result(runtime, input, &collection, self.guard, result).await
            }
            None => result,
        }
    }
}
