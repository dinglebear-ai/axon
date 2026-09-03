use super::*;

fn bulk_context(
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

pub(super) async fn with_bulk_load<F>(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    failure_context: &str,
    processing: F,
) -> anyhow::Result<()>
where
    F: Future<Output = anyhow::Result<()>>,
{
    reserved_call::with_bulk_load(
        runtime,
        bulk_context(input, collection, "begin-bulk-load"),
        bulk_context(input, collection, "finish-bulk-load"),
        collection.collection.clone(),
        failure_context,
        processing,
    )
    .await
}
