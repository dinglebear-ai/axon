//! Pre-generation vector collection setup.

use axon_api::source::{CollectionSpec, PipelinePhase};

use super::{SourcePipelineInput, TargetLocalSourceRuntime};
use crate::reserved_call::{self, ProviderCallContext};

pub(in crate::source::executor) async fn ensure_generation_collection(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
) -> anyhow::Result<()> {
    if !input.plan.request.embed {
        return Ok(());
    }
    reserved_call::ensure_collection(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Upserting,
            input.execution.priority,
            format!("ensure-collection:{}", collection.collection),
        ),
        collection.clone(),
    )
    .await?;
    Ok(())
}

use super::{GenerationAccumulator, SourceGeneration};

pub(super) async fn initialize_accumulator(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
) -> anyhow::Result<GenerationAccumulator> {
    let mut accumulated = GenerationAccumulator::new(
        generation,
        input.plan.job_id,
        input.execution.attempt,
        runtime,
    )
    .await?;
    accumulated.graph_stage = input.graph_stage.clone();
    Ok(accumulated)
}
