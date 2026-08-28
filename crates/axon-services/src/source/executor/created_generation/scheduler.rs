use std::collections::HashMap;
use std::time::Duration;

use axon_api::source::*;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::source::executor::generation_work::{PreparedBatchReceiver, PreparedWorkEnvelope};
use crate::source::executor::progress::PipelineProgress;

// The producer's web acquisition waves are intentionally small and commonly
// arrive hundreds of milliseconds apart. A sub-millisecond microbatch timer
// simply recreates the old one-request-per-wave behavior; this bounded oldest-
// item deadline lets several waves fill one native TEI request while capping
// first-batch latency.
const DEFAULT_FLUSH_DELAY: Duration = Duration::from_millis(1_500);

fn flush_delay() -> Duration {
    std::env::var("AXON_EMBED_SCHEDULER_FLUSH_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| Duration::from_millis(milliseconds.min(5_000)))
        .unwrap_or(DEFAULT_FLUSH_DELAY)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_generation_scheduler(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    mut receiver: PreparedBatchReceiver,
    accumulator: &mut GenerationAccumulator,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let pool_size = runtime.embed_pool_max_inputs.max(1);
    let flush_delay = flush_delay();
    let mut pending = Vec::<PreparedWorkEnvelope>::new();
    let mut pending_chunks = 0_usize;
    let mut pending_bytes = 0_usize;
    let mut deadline = None;
    let mut cumulative = HashMap::<DocumentId, DocumentStatus>::new();
    let mut next_sequence = 0_u64;

    loop {
        if pending_chunks >= pool_size {
            flush_pending(
                runtime,
                input,
                emitter,
                coordinator,
                generation,
                collection.clone(),
                &mut pending,
                accumulator,
                &mut cumulative,
                progress,
                cancel,
            )
            .await?;
            pending_chunks = 0;
            pending_bytes = 0;
            deadline = None;
            continue;
        }

        let received = if let Some(at) = deadline {
            tokio::select! {
                _ = cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
                _ = sleep_until(at) => None,
                envelope = receiver.recv() => envelope,
            }
        } else {
            tokio::select! {
                _ = cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
                envelope = receiver.recv() => envelope,
            }
        };

        match received {
            Some(mut envelope) => {
                anyhow::ensure!(
                    envelope.sequence == next_sequence,
                    "prepared work arrived out of FIFO order"
                );
                next_sequence = next_sequence.saturating_add(1);
                let chunks = envelope
                    .prepared
                    .iter()
                    .map(|document| document.chunks.len())
                    .sum::<usize>();
                progress.add_documents(envelope.prepared.len() as u64);
                let _ = progress.prepared(
                    envelope.prepared.len() as u64,
                    chunks as u64,
                    envelope.is_final,
                );
                accumulator.absorb_pretracked_side_effects(std::mem::replace(
                    &mut envelope.side_effects,
                    crate::source::executor::generation_work::PreparedBatchSideEffects::empty(),
                ));
                pending_chunks = pending_chunks.saturating_add(chunks);
                pending_bytes = pending_bytes.saturating_add(envelope.estimated_bytes);
                pending.push(envelope);
                deadline.get_or_insert_with(|| Instant::now() + flush_delay);
            }
            None if pending.is_empty() => break,
            None => {
                flush_pending(
                    runtime,
                    input,
                    emitter,
                    coordinator,
                    generation,
                    collection.clone(),
                    &mut pending,
                    accumulator,
                    &mut cumulative,
                    progress,
                    cancel,
                )
                .await?;
                pending_chunks = 0;
                pending_bytes = 0;
                deadline = None;
                if receiver.is_channel_closed() {
                    break;
                }
            }
        }
    }
    tracing::debug!(pending_bytes, "generation scheduler drained prepared work");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn flush_pending(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    pending: &mut Vec<PreparedWorkEnvelope>,
    accumulator: &mut GenerationAccumulator,
    cumulative: &mut HashMap<DocumentId, DocumentStatus>,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let mut prepared = Vec::new();
    for envelope in pending.iter_mut() {
        prepared.append(&mut envelope.prepared);
    }
    for pool in vectorize::batching::chunk_batches(prepared, runtime.embed_pool_max_inputs) {
        let result = vectorize::vectorize_prepared_pool(
            runtime,
            input,
            generation,
            collection.clone(),
            emitter,
            coordinator,
            pool,
            cumulative,
            progress,
            cancel,
        )
        .await?;
        accumulator.absorb_vectorized(result);
    }
    // Dropping envelopes only after vector upsert, cumulative status writes,
    // and accumulator absorption releases their chunk/byte permits.
    pending.clear();
    Ok(())
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
