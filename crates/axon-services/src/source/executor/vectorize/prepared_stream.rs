//! Rolling bounded admission with ordered durable publication.
use super::*;
use crate::source::executor::generation_state::GenerationAccumulator;
use crate::source::executor::generation_work::{
    PreparedBatchReceiver, PreparedBatchSideEffects, PreparedWorkEnvelope,
};
use futures_util::{StreamExt, stream::FuturesUnordered};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

struct PoolWork {
    documents: Vec<PreparedDocument>,
    is_final: bool,
    // Retain source-work admission until every pool sharing the envelopes has
    // been durably accounted. Completed out-of-order pools remain charged.
    _envelopes: Arc<Vec<PreparedWorkEnvelope>>,
}

struct CompletedPool {
    sequence: u64,
    work: PoolWork,
    embedding: anyhow::Result<Option<EmbeddingResult>>,
}

type WorkFuture<'a> = Pin<Box<dyn Future<Output = CompletedPool> + Send + 'a>>;

#[derive(Default)]
struct StreamState<'a> {
    pending: Vec<PreparedWorkEnvelope>,
    pending_chunks: usize,
    deadline: Option<tokio::time::Instant>,
    queued: VecDeque<PoolWork>,
    flights: FuturesUnordered<WorkFuture<'a>>,
    completed: BTreeMap<u64, CompletedPool>,
    received: u64,
    admitted: u64,
    published: u64,
    closed: bool,
}

#[derive(Clone, Copy)]
struct StreamContext<'a, 'input> {
    runtime: &'a TargetLocalSourceRuntime,
    input: &'a SourcePipelineInput<'input>,
    emitter: &'a SourceEventEmitter,
    coordinator: &'a ProgressCoordinator,
    cancel: &'a CancellationToken,
}

impl PreparedPoolVectorizer {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::source::executor) async fn run_stream(
        &mut self,
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
        let context = StreamContext {
            runtime,
            input,
            emitter,
            coordinator,
            cancel,
        };
        let result = self
            .run_stream_inner(context, collection, receiver, accumulator, progress)
            .await;
        if let Some(collection) = self.bulk_collection.take() {
            finish_bulk_result(runtime, input, &collection, self.bulk_guard.take(), result).await
        } else {
            result
        }
    }

    async fn run_stream_inner(
        &mut self,
        context: StreamContext<'_, '_>,
        mut collection: CollectionSpec,
        mut receiver: PreparedBatchReceiver,
        accumulator: &mut GenerationAccumulator,
        progress: &mut PipelineProgress,
    ) -> anyhow::Result<()> {
        let mut state = StreamState::default();
        // A serialized mode has no speculative work alive during publication.
        let window = if context.runtime.vector_upsert_embed_overlap {
            3
        } else {
            1
        };
        loop {
            anyhow::ensure!(
                !context.cancel.is_cancelled(),
                "generation scheduler canceled"
            );
            if !state.pending.is_empty() && flush_due(&state, context.runtime.embed_pool_max_inputs)
            {
                queue_pending(&mut state, context.runtime.embed_pool_max_inputs);
            }
            while state.flights.len() + state.completed.len() < window {
                let Some(work) = state.queued.pop_front() else {
                    break;
                };
                if context.input.plan.request.embed
                    && work.documents.iter().any(|doc| !doc.chunks.is_empty())
                {
                    collection = poll_with_work(
                        self.ensure_pool_providers(context.runtime, context.input),
                        &mut state.flights,
                        &mut state.completed,
                        context.cancel,
                    )
                    .await?;
                }
                admit_work(context, &mut state, work, progress).await?;
            }
            if let Some(completed) = state.completed.remove(&state.published) {
                let counts = progress.upserting_counts();
                let providers_ready = self.providers_ready || !context.input.plan.request.embed;
                let publication = self.publish_completed(
                    context,
                    collection.clone(),
                    completed,
                    accumulator,
                    progress,
                );
                let deferred = drive_publication(
                    publication,
                    context,
                    &mut state,
                    &mut receiver,
                    window,
                    providers_ready,
                    counts,
                )
                .await?;
                // Intake and batching accounting is deferred while publication
                // borrows the progress/accumulator. Apply it after the durable
                // checkpoint; speculative providers heartbeat Upserting only.
                for (effects, documents, chunks, final_item) in deferred.prepared {
                    progress.add_documents(documents);
                    let _ = progress.prepared(documents, chunks, final_item);
                    poll_with_work(
                        accumulator.absorb_pretracked_side_effects(effects),
                        &mut state.flights,
                        &mut state.completed,
                        context.cancel,
                    )
                    .await?;
                }
                let _ = progress.batched(deferred.batched);
                state.published += 1;
                continue;
            }
            if state.closed
                && state.pending.is_empty()
                && state.queued.is_empty()
                && state.flights.is_empty()
            {
                anyhow::ensure!(
                    state.completed.is_empty(),
                    "prepared pool sequence gap before publication"
                );
                break;
            }
            let can_receive = !state.closed
                && state.queued.is_empty()
                && state.flights.len() + state.completed.len() < window;
            let deadline = state.deadline;
            tokio::select! {
                _ = context.cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
                completed = state.flights.next(), if !state.flights.is_empty() => {
                    let completed = completed.expect("nonempty flight set");
                    state.completed.insert(completed.sequence, completed);
                }
                envelope = receiver.recv(), if can_receive => {
                    match envelope {
                        Some(envelope) => accept_work(context, &mut state, envelope, accumulator, progress).await?,
                        None => state.closed = true,
                    }
                }
                _ = wait_deadline(deadline), if deadline.is_some() => queue_pending(&mut state, context.runtime.embed_pool_max_inputs),
            }
        }
        Ok(())
    }

    async fn publish_completed(
        &mut self,
        context: StreamContext<'_, '_>,
        collection: CollectionSpec,
        completed: CompletedPool,
        accumulator: &mut GenerationAccumulator,
        progress: &mut PipelineProgress,
    ) -> anyhow::Result<()> {
        let CompletedPool {
            work, embedding, ..
        } = completed;
        let result = if let Some(mut embedding) = embedding? {
            pipeline::finish_embedding(context.coordinator, progress, &embedding).await;
            let documents = pipeline::release_staged_candidates(context.input, work.documents);
            let built = pipeline::build_vector_batch(
                documents,
                collection,
                &mut embedding,
                context.emitter,
                context.coordinator,
                progress,
                work.is_final,
            )
            .await?;
            publish_built_batch(
                context.runtime,
                context.input,
                built,
                context.emitter,
                context.coordinator,
                progress,
            )
            .await?
        } else {
            let mut result = statuses_only(work.documents, DocumentLifecycleStatus::Prepared);
            if context.input.graph_stage.is_some() {
                result.graph_candidates.clear();
            }
            result
        };
        self.checkpoint(context.runtime, &result).await?;
        accumulator.absorb_vectorized(result);
        // `work._envelopes` drops only after checkpoint and accounting.
        Ok(())
    }
}

fn flush_due(state: &StreamState<'_>, pool_size: usize) -> bool {
    state.closed
        || state.pending_chunks >= pool_size.max(1)
        || state.pending.last().is_some_and(|work| work.is_final)
        || state
            .deadline
            .is_some_and(|deadline| deadline <= tokio::time::Instant::now())
}

fn queue_pending(state: &mut StreamState<'_>, pool_size: usize) {
    if state.pending.is_empty() {
        return;
    }
    let mut documents = Vec::new();
    let is_final = state.pending.iter().any(|envelope| envelope.is_final);
    for envelope in &mut state.pending {
        documents.append(&mut envelope.prepared);
    }
    let envelopes = Arc::new(std::mem::take(&mut state.pending));
    let mut pools = chunk_batches(documents, pool_size);
    if pools.is_empty() {
        pools.push(Vec::new());
    }
    let last = pools.len() - 1;
    state.queued.extend(
        pools
            .into_iter()
            .enumerate()
            .map(|(index, documents)| PoolWork {
                documents,
                is_final: is_final && index == last,
                _envelopes: Arc::clone(&envelopes),
            }),
    );
    state.pending_chunks = 0;
    state.deadline = None;
}

async fn wait_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn accept_work(
    context: StreamContext<'_, '_>,
    state: &mut StreamState<'_>,
    mut envelope: PreparedWorkEnvelope,
    accumulator: &mut GenerationAccumulator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        envelope.sequence == state.received,
        "prepared work arrived out of FIFO order"
    );
    state.received += 1;
    let chunks = envelope
        .prepared
        .iter()
        .map(|doc| doc.chunks.len())
        .sum::<usize>();
    progress.add_documents(envelope.prepared.len() as u64);
    let _ = progress.prepared(
        envelope.prepared.len() as u64,
        chunks as u64,
        envelope.is_final,
    );
    let effects = std::mem::replace(
        &mut envelope.side_effects,
        PreparedBatchSideEffects::empty(),
    );
    poll_with_work(
        accumulator.absorb_pretracked_side_effects(effects),
        &mut state.flights,
        &mut state.completed,
        context.cancel,
    )
    .await?;
    state.pending_chunks += batching::charged_chunk_count(&envelope.prepared);
    state.deadline.get_or_insert_with(|| {
        tokio::time::Instant::now() + context.runtime.embed_scheduler_flush_delay
    });
    state.pending.push(envelope);
    Ok(())
}

async fn admit_work<'a>(
    context: StreamContext<'a, '_>,
    state: &mut StreamState<'a>,
    work: PoolWork,
    progress: &mut PipelineProgress,
) -> anyhow::Result<()> {
    let sequence = state.admitted;
    state.admitted += 1;
    let chunks = work
        .documents
        .iter()
        .map(|doc| doc.chunks.len())
        .sum::<usize>();
    if !context.input.plan.request.embed || chunks == 0 {
        start_work(
            context,
            state,
            work,
            sequence,
            PipelinePhase::Preparing,
            progress.preparing_counts(),
        );
        return Ok(());
    }
    let counts = poll_with_work(
        async {
            context
                .coordinator
                .report(
                    context.emitter,
                    PipelinePhase::Batching,
                    progress.batched(chunks as u64),
                    "batching prepared chunks",
                )
                .await;
            Ok(pipeline::begin_embedding(context.emitter, context.coordinator, progress).await)
        },
        &mut state.flights,
        &mut state.completed,
        context.cancel,
    )
    .await?;
    let phase = if context.input.plan.request.embed && chunks > 0 {
        PipelinePhase::Embedding
    } else {
        PipelinePhase::Preparing
    };
    start_work(context, state, work, sequence, phase, counts);
    Ok(())
}

fn start_work<'a>(
    context: StreamContext<'a, '_>,
    state: &mut StreamState<'a>,
    work: PoolWork,
    sequence: u64,
    phase: PipelinePhase,
    counts: StageCounts,
) {
    let chunks = work
        .documents
        .iter()
        .map(|doc| doc.chunks.len())
        .sum::<usize>();
    state.flights.push(Box::pin(async move {
        let embedding = if context.input.plan.request.embed && chunks > 0 {
            pipeline::call_embedding(
                context.runtime,
                context.input,
                &work.documents,
                phase,
                counts,
            )
            .await
            .map(Some)
        } else {
            crate::source::graph::staging::stage_prepared(
                context.runtime,
                context.input,
                &work.documents,
                phase,
            )
            .await
            .map(|()| None)
        };
        CompletedPool {
            sequence,
            work,
            embedding,
        }
    }));
}

async fn poll_with_work<T>(
    work: impl Future<Output = anyhow::Result<T>>,
    flights: &mut FuturesUnordered<WorkFuture<'_>>,
    completed: &mut BTreeMap<u64, CompletedPool>,
    cancel: &CancellationToken,
) -> anyhow::Result<T> {
    tokio::pin!(work);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("generation scheduler canceled"),
            result = &mut work => return result,
            result = flights.next(), if !flights.is_empty() => {
                let result = result.expect("nonempty flight set");
                completed.insert(result.sequence, result);
            }
        }
    }
}

#[path = "prepared_stream_publication.rs"]
mod publication;
use publication::drive_publication;
