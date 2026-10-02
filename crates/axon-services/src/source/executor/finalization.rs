//! Independent cleanup and failure marking for every error after generation creation.
use super::*;

pub(super) async fn finalize_failed_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: SourceGeneration,
    mut error: anyhow::Error,
) -> anyhow::Error {
    if error
        .downcast_ref::<ApiError>()
        .is_some_and(|e| e.code.to_string() == "graph.publication_commit_unknown")
    {
        return error;
    }
    // A detached atomic publisher retains this gate until commit/rollback settles.
    let settlement = runtime.publication_settlement_gate.lock().await;
    let writer = runtime.sqlite_write_gate.lock().await;
    let committed = match runtime
        .ledger
        .committed_generation(generation.source_id.clone())
        .await
    {
        Ok(current) => Some(current.as_ref() == Some(&generation.generation)),
        Err(lookup_error) => {
            error = error.context(format!(
                "could not verify committed generation; vector cleanup skipped: {lookup_error}"
            ));
            None
        }
    };
    drop(writer);
    drop(settlement);
    if committed == Some(true) {
        return error;
    }
    // Unknown provenance must never authorize deletion. The ledger's guarded
    // failure transition can still reject a generation already published.
    if committed == Some(false)
        && input.plan.request.embed
        && let Err(cleanup_error) = publish::cleanup_failed_generation_vectors(
            runtime,
            input,
            input.collection,
            &generation,
        )
        .await
    {
        error = error.context(format!(
            "failed-generation vector cleanup also failed: {cleanup_error:#}"
        ));
    }
    if let Err(fail_error) = runtime.ledger.fail_generation(generation).await {
        error = error.context(format!(
            "also failed to mark source generation failed: {fail_error}"
        ));
    }
    error
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../graph/publication_settlement_tests.rs"]
mod publication_settlement_tests;
