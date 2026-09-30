//! Reserve a source generation that cannot collide with imported vector points.

use axon_api::source::*;
use axon_ledger::store::LedgerStore;

use super::SourcePipelineInput;
use crate::context::TargetLocalSourceRuntime;

pub(super) async fn reserve_unoccupied_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    source_id: &SourceId,
) -> anyhow::Result<(SourceGeneration, Vec<SourceGenerationId>)> {
    // Imported vectors can predate the ledger. Never reuse their generation
    // number: a failed publish would otherwise roll back both old and new points.
    let mut occupied_generations = Vec::new();
    let generation = loop {
        anyhow::ensure!(
            occupied_generations.len() < 32,
            "source {} has at least 32 vector generations absent from its ledger; indexing stopped before writing vectors. Inspect this source's Qdrant points and ledger history before retrying",
            source_id.0
        );
        let candidate = runtime.ledger.create_generation(source_id.clone()).await?;
        let count = if input.plan.request.embed {
            crate::reserved_call::count_generation_points(
                runtime,
                crate::reserved_call::ProviderCallContext::for_phase(
                    input.plan.job_id,
                    input.execution.attempt,
                    PipelinePhase::Publishing,
                    input.execution.priority,
                    format!("check-generation:{}", candidate.generation.0),
                ),
                input.collection.to_string(),
                source_id.clone(),
                candidate.generation.clone(),
            )
            .await
        } else {
            Ok(0)
        };
        match count {
            Ok(0) => break candidate,
            Ok(_) => {
                runtime.ledger.fail_generation(candidate.clone()).await?;
                occupied_generations.push(candidate.generation);
            }
            Err(error) => {
                runtime.ledger.fail_generation(candidate).await?;
                return Err(error.into());
            }
        }
    };
    Ok((generation, occupied_generations))
}
