//! Bounded late intake while one ordered pool is being published.
use super::*;

type DeferredPreparation = (PreparedBatchSideEffects, u64, u64, bool);

#[derive(Default)]
pub(super) struct DeferredProgress {
    pub(super) prepared: Vec<DeferredPreparation>,
    pub(super) batched: u64,
}

pub(super) async fn drive_publication<'a>(
    publication: impl Future<Output = anyhow::Result<()>>,
    context: StreamContext<'a, '_>,
    state: &mut StreamState<'a>,
    receiver: &mut PreparedBatchReceiver,
    window: usize,
    providers_ready: bool,
    counts: StageCounts,
) -> anyhow::Result<DeferredProgress> {
    tokio::pin!(publication);
    let mut deferred = DeferredProgress::default();
    loop {
        // The publishing pool occupies one slot until its durable checkpoint.
        // Count completed out-of-order results as well as active provider calls.
        let available = if providers_ready {
            window.saturating_sub(1)
        } else {
            0
        };
        if available > 0 {
            if !state.pending.is_empty() && flush_due(state, context.runtime.embed_pool_max_inputs)
            {
                queue_pending(state, context.runtime.embed_pool_max_inputs);
            }
            while state.flights.len() + state.completed.len() < available {
                let Some(work) = state.queued.pop_front() else {
                    break;
                };
                deferred.batched += work
                    .documents
                    .iter()
                    .map(|doc| doc.chunks.len() as u64)
                    .sum::<u64>();
                let sequence = state.admitted;
                state.admitted += 1;
                start_work(
                    context,
                    state,
                    work,
                    sequence,
                    PipelinePhase::Upserting,
                    counts.clone(),
                );
            }
        }
        let receive = available > 0
            && !state.closed
            && state.queued.is_empty()
            && state.flights.len() + state.completed.len() < available;
        let deadline = state.deadline;
        tokio::select! {
            // Prefer a finished durable write over a simultaneous cancellation.
            // It must reach accounting before cancellation can terminate intake.
            biased;
            result = &mut publication => { result?; return Ok(deferred); }
            _ = context.cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
            completed = state.flights.next(), if !state.flights.is_empty() => {
                let completed = completed.expect("nonempty flight set");
                state.completed.insert(completed.sequence, completed);
            }
            envelope = receiver.recv(), if receive => {
                match envelope {
                    Some(mut envelope) => {
                        anyhow::ensure!(envelope.sequence == state.received, "prepared work arrived out of FIFO order");
                        state.received += 1;
                        let documents = envelope.prepared.len() as u64;
                        let chunks = envelope.prepared.iter().map(|doc| doc.chunks.len() as u64).sum();
                        let effects = std::mem::replace(&mut envelope.side_effects, PreparedBatchSideEffects::empty());
                        deferred.prepared.push((effects, documents, chunks, envelope.is_final));
                        state.pending_chunks += batching::charged_chunk_count(&envelope.prepared);
                        state.deadline.get_or_insert_with(|| tokio::time::Instant::now() + context.runtime.embed_scheduler_flush_delay);
                        state.pending.push(envelope);
                    }
                    None => state.closed = true,
                }
            }
            _ = wait_deadline(deadline), if available > 0 && deadline.is_some() => queue_pending(state, context.runtime.embed_pool_max_inputs),
        }
    }
}
