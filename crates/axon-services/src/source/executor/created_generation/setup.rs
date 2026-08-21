//! Pre-generation vector collection setup.

use axon_api::source::{CollectionSpec, PipelinePhase};

use super::{SourcePipelineInput, TargetLocalSourceRuntime};
use crate::reserved_call::{self, ProviderCallContext};

pub(super) async fn ensure_generation_collection(
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
