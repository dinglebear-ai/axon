use axon_api::source::*;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::source::executor::generation_work::PreparedBatchReceiver;
use crate::source::executor::progress::PipelineProgress;

// The producer's web acquisition waves are intentionally small and commonly
// arrive hundreds of milliseconds apart. A sub-millisecond microbatch timer
// simply recreates the old one-request-per-wave behavior; this bounded oldest-
// item deadline lets several waves fill one native TEI request while capping
// first-batch latency.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_generation_scheduler(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    collection: CollectionSpec,
    receiver: PreparedBatchReceiver,
    accumulator: &mut GenerationAccumulator,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    vectorize::PreparedPoolVectorizer::default()
        .run_stream(
            runtime,
            input,
            emitter,
            coordinator,
            collection,
            receiver,
            accumulator,
            progress,
            cancel,
        )
        .await
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
