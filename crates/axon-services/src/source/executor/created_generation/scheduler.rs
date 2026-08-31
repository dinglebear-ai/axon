use axon_api::source::*;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::source::executor::generation_work::{PreparedBatchReceiver, PreparedWorkEnvelope};
use crate::source::executor::progress::PipelineProgress;
use crate::source::executor::vectorize::batching::charged_chunk_count;

const OUTER_POOL_CONCURRENCY: usize = 3;

fn should_flush(
    pending_chunks: usize,
    pending_envelopes: usize,
    pool_size: usize,
    is_final: bool,
    closed: bool,
) -> bool {
    is_final
        || closed
        || pending_chunks >= pool_size.max(1).saturating_mul(OUTER_POOL_CONCURRENCY)
        || pending_envelopes >= OUTER_POOL_CONCURRENCY
}

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
    mut receiver: PreparedBatchReceiver,
    accumulator: &mut GenerationAccumulator,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let pool_size = runtime.embed_pool_max_inputs.max(1);
    let mut pending = Vec::<PreparedWorkEnvelope>::new();
    let mut pending_chunks = 0_usize;
    let mut pending_bytes = 0_usize;
    let mut vectorizer = vectorize::PreparedPoolVectorizer::default();
    let mut next_sequence = 0_u64;

    loop {
        if should_flush(pending_chunks, pending.len(), pool_size, false, false) {
            flush_pending(
                runtime,
                input,
                emitter,
                coordinator,
                collection.clone(),
                &mut pending,
                accumulator,
                &mut vectorizer,
                progress,
                cancel,
            )
            .await?;
            pending_chunks = 0;
            pending_bytes = 0;
            continue;
        }

        let received = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
            envelope = receiver.recv() => envelope,
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
                ))?;
                pending_chunks =
                    pending_chunks.saturating_add(charged_chunk_count(&envelope.prepared));
                pending_bytes = pending_bytes.saturating_add(envelope.estimated_bytes);
                pending.push(envelope);
                if should_flush(
                    pending_chunks,
                    pending.len(),
                    pool_size,
                    pending.last().is_some_and(|envelope| envelope.is_final),
                    false,
                ) {
                    flush_pending(
                        runtime,
                        input,
                        emitter,
                        coordinator,
                        collection.clone(),
                        &mut pending,
                        accumulator,
                        &mut vectorizer,
                        progress,
                        cancel,
                    )
                    .await?;
                    pending_chunks = 0;
                    pending_bytes = 0;
                }
            }
            None if pending.is_empty() => break,
            None => {
                flush_pending(
                    runtime,
                    input,
                    emitter,
                    coordinator,
                    collection.clone(),
                    &mut pending,
                    accumulator,
                    &mut vectorizer,
                    progress,
                    cancel,
                )
                .await?;
                pending_chunks = 0;
                pending_bytes = 0;
                if receiver.is_channel_closed() {
                    break;
                }
            }
        }
    }
    if let Some(result) = vectorizer
        .finish(runtime, input, emitter, coordinator, progress)
        .await?
    {
        accumulator.absorb_vectorized(result);
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
    collection: CollectionSpec,
    pending: &mut Vec<PreparedWorkEnvelope>,
    accumulator: &mut GenerationAccumulator,
    vectorizer: &mut vectorize::PreparedPoolVectorizer,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let mut prepared = Vec::new();
    let is_final_group = pending.iter().any(|envelope| envelope.is_final);
    for envelope in pending.iter_mut() {
        prepared.append(&mut envelope.prepared);
    }
    let pools = vectorize::batching::chunk_batches(prepared, runtime.embed_pool_max_inputs);
    for outcome in vectorizer
        .push_many(
            runtime,
            input,
            collection,
            emitter,
            coordinator,
            pools,
            is_final_group,
            progress,
            cancel,
        )
        .await?
    {
        match outcome {
            vectorize::PushOutcome::Published(result) => {
                accumulator.absorb_vectorized(result);
                // The previously built pool has now been durably published and
                // checkpointed. Its source-work permits may be released.
            }
            vectorize::PushOutcome::StatusesOnly(result) => accumulator.absorb_vectorized(result),
            vectorize::PushOutcome::NoPublication => {}
        }
    }
    // Prepared payload ownership has moved into the built vector batch (or was
    // checkpointed as statuses). Release acquisition-stage permits here rather
    // than coupling producer progress to later provider durability.
    pending.clear();
    Ok(())
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
