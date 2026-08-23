//! Durable, best-effort progress coordination for the unified source runner.
//!
//! Adapter observations are batch-local. This module validates them, converts
//! them to generation-global StageCounts, and persists complete snapshots.
//! Progress failures are logged and swallowed so observation cannot fail an
//! otherwise-successful source operation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axon_adapters::{AcquisitionProgress, AcquisitionProgressSink};
use axon_api::source::*;
use axon_jobs::boundary::{JobStore, Result as JobResult};
use tokio::sync::Mutex;

use super::{SourceEventEmitter, SourcePipelineInput, TargetLocalSourceRuntime};
use crate::source::foreground_progress::ForegroundProgressSender;

const DEFAULT_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[async_trait]
trait ProgressStatusWriter: Send + Sync {
    async fn update(&self, update: JobStatusUpdate) -> JobResult<()>;
}

struct JobStoreProgressWriter(Arc<dyn JobStore>);

#[cfg(test)]
struct NoopProgressWriter;

#[async_trait]
impl ProgressStatusWriter for JobStoreProgressWriter {
    async fn update(&self, update: JobStatusUpdate) -> JobResult<()> {
        self.0.update_status(update).await
    }
}

#[cfg(test)]
#[async_trait]
impl ProgressStatusWriter for NoopProgressWriter {
    async fn update(&self, _update: JobStatusUpdate) -> JobResult<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct CoordinatorState {
    phase_counts: Vec<(PipelinePhase, StageCounts)>,
    /// The phase most recently published as a transition (`report`/
    /// `checkpoint`). Count-only checkpoints reuse it so they never regress
    /// the externally visible phase.
    current_phase: Option<PipelinePhase>,
    #[cfg(test)]
    phase_history: Vec<PipelinePhase>,
}

/// Orchestration-owned publisher for complete, monotonic source-job snapshots.
#[derive(Clone)]
pub(super) struct ProgressCoordinator {
    writer: Arc<dyn ProgressStatusWriter>,
    job_id: JobId,
    source_id: SourceId,
    adapter: String,
    state: Arc<Mutex<CoordinatorState>>,
    interval: Duration,
    foreground: Option<ForegroundProgressSender>,
}

impl ProgressCoordinator {
    pub(super) fn new(runtime: &TargetLocalSourceRuntime, input: &SourcePipelineInput<'_>) -> Self {
        Self {
            writer: Arc::new(JobStoreProgressWriter(runtime.jobs.clone())),
            job_id: input.plan.job_id,
            source_id: input.plan.route.source.source_id.clone(),
            adapter: input.plan.route.adapter.name.clone(),
            state: Arc::new(Mutex::new(CoordinatorState::default())),
            interval: DEFAULT_PROGRESS_INTERVAL,
            foreground: input.execution.foreground.clone(),
        }
    }

    #[cfg(test)]
    pub(super) fn test_noop() -> Self {
        Self::with_writer(
            Arc::new(NoopProgressWriter),
            JobId::new(uuid::Uuid::from_u128(1)),
            SourceId::new("src-overlap-test"),
            "test",
            Duration::ZERO,
        )
    }

    #[cfg(test)]
    pub(super) async fn recorded_phase_order(&self) -> Vec<PipelinePhase> {
        self.state.lock().await.phase_history.clone()
    }

    #[cfg(test)]
    fn with_writer(
        writer: Arc<dyn ProgressStatusWriter>,
        job_id: JobId,
        source_id: SourceId,
        adapter: impl Into<String>,
        interval: Duration,
    ) -> Self {
        Self::with_writer_and_foreground(writer, job_id, source_id, adapter, interval, None)
    }

    #[cfg(test)]
    fn with_writer_and_foreground(
        writer: Arc<dyn ProgressStatusWriter>,
        job_id: JobId,
        source_id: SourceId,
        adapter: impl Into<String>,
        interval: Duration,
        foreground: Option<ForegroundProgressSender>,
    ) -> Self {
        Self {
            writer,
            job_id,
            source_id,
            adapter: adapter.into(),
            state: Arc::new(Mutex::new(CoordinatorState::default())),
            interval,
            foreground,
        }
    }

    /// Publish a phase transition and its complete active coordinate system.
    pub(super) async fn report(
        &self,
        emitter: &SourceEventEmitter,
        phase: PipelinePhase,
        counts: StageCounts,
        message: &str,
    ) {
        let (counts, persisted) = self.persist(phase, counts, message).await;
        if persisted {
            emitter
                .running_with_counts(phase, message, Some(counts))
                .await;
        } else {
            emitter.running(phase, message).await;
        }
    }

    /// Persist a progress checkpoint without emitting a duplicate source event.
    pub(super) async fn checkpoint(
        &self,
        phase: PipelinePhase,
        counts: StageCounts,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.persist(phase, counts, &message).await;
    }

    /// Persist a count-only checkpoint for `phase` without regressing the
    /// externally published phase: the counts are stored under `phase` (so
    /// later `phase` snapshots continue from them) while the durable update
    /// keeps the currently published phase. Used by speculative prefetch
    /// acquisition, whose Fetching counts would otherwise freeze at the first
    /// batch (2026-08-23 adversarial pipeline review, low: fetch-count
    /// freeze).
    async fn checkpoint_counts(
        &self,
        phase: PipelinePhase,
        counts: StageCounts,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.persist_with_phase_floor(phase, counts, &message, true)
            .await;
    }

    #[cfg(test)]
    pub(super) async fn latest_counts(&self, phase: PipelinePhase) -> Option<StageCounts> {
        self.state
            .lock()
            .await
            .phase_counts
            .iter()
            .find_map(|(stored_phase, counts)| (*stored_phase == phase).then(|| counts.clone()))
    }

    pub(super) fn acquisition_batch(
        &self,
        generation_items_total: u64,
        batch_items_total: u64,
        items_offset: u64,
        documents_offset: u64,
        publish_phase: bool,
    ) -> AcquisitionBatchProgress<'_> {
        AcquisitionBatchProgress {
            coordinator: self,
            generation_items_total,
            batch_items_total,
            items_offset,
            documents_offset,
            publish_phase,
            state: Mutex::new(BatchProgressState::default()),
        }
    }

    async fn persist(
        &self,
        phase: PipelinePhase,
        counts: StageCounts,
        message: &str,
    ) -> (StageCounts, bool) {
        self.persist_with_phase_floor(phase, counts, message, false)
            .await
    }

    async fn persist_with_phase_floor(
        &self,
        phase: PipelinePhase,
        counts: StageCounts,
        message: &str,
        keep_published_phase: bool,
    ) -> (StageCounts, bool) {
        let (published_phase, counts) = {
            let mut state = self.state.lock().await;
            let published_phase = if keep_published_phase {
                state.current_phase.unwrap_or(phase)
            } else {
                state.current_phase = Some(phase);
                phase
            };
            #[cfg(test)]
            if state.phase_history.last() != Some(&published_phase) {
                state.phase_history.push(published_phase);
            }
            (
                published_phase,
                normalize_phase_counts(&mut state.phase_counts, phase, counts),
            )
        };
        let update = JobStatusUpdate {
            job_id: self.job_id,
            source_id: Some(self.source_id.clone()),
            status: LifecycleStatus::Running,
            phase: published_phase,
            stage_id: None,
            counts: Some(counts.clone()),
            current: Some(ProgressCurrent {
                source_item_key: None,
                document_id: None,
                chunk_id: None,
                adapter: Some(self.adapter.clone()),
                provider: None,
                message: Some(message.to_string()),
            }),
            message: Some(message.to_string()),
            error: None,
        };
        let persisted = match self.writer.update(update.clone()).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(
                    job_id = %self.job_id.0,
                    source_id = %self.source_id.0,
                    phase = ?phase,
                    adapter = %self.adapter,
                    items_done = counts.items_done,
                    documents_done = counts.documents_done,
                    chunks_done = counts.chunks_done,
                    error = %error,
                    "failed to persist source progress"
                );
                false
            }
        };
        if let Some(foreground) = &self.foreground {
            foreground.snapshot(update);
        }
        (counts, persisted)
    }
}

#[derive(Debug, Default)]
struct BatchProgressState {
    items_done: u64,
    documents_done: u64,
    last_write: Option<Instant>,
}

/// Batch-local adapter sink projected into generation-global acquisition counts.
pub(super) struct AcquisitionBatchProgress<'a> {
    coordinator: &'a ProgressCoordinator,
    generation_items_total: u64,
    batch_items_total: u64,
    items_offset: u64,
    documents_offset: u64,
    publish_phase: bool,
    state: Mutex<BatchProgressState>,
}

impl AcquisitionBatchProgress<'_> {
    /// Force the runner-owned final snapshot even when the adapter emitted none.
    pub(super) async fn complete(&self, documents_done: u64) {
        self.record(
            AcquisitionProgress {
                items_total: self.batch_items_total,
                items_done: self.batch_items_total,
                documents_done,
            },
            true,
        )
        .await;
    }

    async fn record(&self, progress: AcquisitionProgress, force: bool) {
        if progress.items_total != self.batch_items_total {
            tracing::warn!(
                job_id = %self.coordinator.job_id.0,
                adapter = %self.coordinator.adapter,
                reported_total = progress.items_total,
                expected_total = self.batch_items_total,
                "adapter reported mismatched acquisition progress total"
            );
        }
        let (items_done, documents_done, should_write) = {
            let mut state = self.state.lock().await;
            let items_done = progress.items_done.min(self.batch_items_total);
            let documents_done = progress.documents_done.min(items_done);
            state.items_done = state.items_done.max(items_done);
            state.documents_done = state
                .documents_done
                .max(documents_done)
                .min(state.items_done);
            let now = Instant::now();
            let due = state
                .last_write
                .is_none_or(|last| now.duration_since(last) >= self.coordinator.interval);
            let should_write = force || due || state.items_done >= self.batch_items_total;
            if should_write {
                state.last_write = Some(now);
            }
            (state.items_done, state.documents_done, should_write)
        };
        if !should_write {
            return;
        }
        let global_items = self
            .items_offset
            .saturating_add(items_done)
            .min(self.generation_items_total);
        let global_documents = self
            .documents_offset
            .saturating_add(documents_done)
            .min(self.generation_items_total);
        let counts = stage_counts(
            Some(self.generation_items_total),
            global_items,
            Some(self.generation_items_total),
            global_documents,
            None,
            0,
        );
        let message = format!(
            "acquired {global_items}/{} source items",
            self.generation_items_total
        );
        if self.publish_phase {
            self.coordinator
                .checkpoint(PipelinePhase::Fetching, counts, message)
                .await;
        } else {
            // Speculative prefetch acquisition: keep the Fetching counts
            // advancing without regressing the published phase of the batch
            // still being processed.
            self.coordinator
                .checkpoint_counts(PipelinePhase::Fetching, counts, message)
                .await;
        }
    }
}

#[async_trait]
impl AcquisitionProgressSink for AcquisitionBatchProgress<'_> {
    async fn report(&self, progress: AcquisitionProgress) {
        self.record(progress, false).await;
    }
}

/// Generation-global downstream counters accumulated across bounded batches.
///
/// A phase total remains `None` while later bounded batches can still expand
/// it. The runner marks each coordinate final exactly once, after which the
/// known denominator is stable for every subsequent snapshot in that phase.
#[derive(Debug, Default)]
pub(super) struct PipelineProgress {
    documents_total: u64,
    documents_prepared: u64,
    documents_final: bool,
    chunks_total: u64,
    chunks_batched: u64,
    chunks_embedded: u64,
    chunks_final: bool,
    vectors_total: u64,
    vectors_built: u64,
    vectors_upserted: u64,
    vectors_final: bool,
}

impl PipelineProgress {
    pub(super) fn add_documents(&mut self, documents: u64) {
        self.documents_total = self.documents_total.saturating_add(documents);
    }

    pub(super) fn finish_documents(&mut self) {
        self.documents_final = true;
    }

    pub(super) fn preparing_counts(&self) -> StageCounts {
        stage_counts(
            self.documents_total(),
            self.documents_prepared,
            self.documents_total(),
            self.documents_prepared,
            self.chunks_total(),
            self.chunks_total,
        )
    }

    pub(super) fn prepared(
        &mut self,
        documents: u64,
        chunks: u64,
        chunks_final: bool,
    ) -> StageCounts {
        self.documents_prepared = self.documents_prepared.saturating_add(documents);
        self.chunks_total = self.chunks_total.saturating_add(chunks);
        self.chunks_final |= chunks_final;
        stage_counts(
            self.documents_total(),
            self.documents_prepared,
            self.documents_total(),
            self.documents_prepared,
            self.chunks_total(),
            self.chunks_total,
        )
    }

    pub(super) fn batched(&mut self, chunks: u64) -> StageCounts {
        self.chunks_batched = self.chunks_batched.saturating_add(chunks);
        stage_counts(
            self.documents_total(),
            self.documents_prepared,
            self.documents_total(),
            self.documents_prepared,
            self.chunks_total(),
            self.chunks_batched,
        )
    }

    pub(super) fn embedding_counts(&self) -> StageCounts {
        stage_counts(
            self.documents_total(),
            self.documents_prepared,
            self.documents_total(),
            self.documents_prepared,
            self.chunks_total(),
            self.chunks_embedded,
        )
    }

    pub(super) fn embedded(&mut self, chunks: u64) -> StageCounts {
        self.chunks_embedded = self.chunks_embedded.saturating_add(chunks);
        self.embedding_counts()
    }

    pub(super) fn vectorized(&mut self, points: u64, vectors_final: bool) -> StageCounts {
        self.vectors_total = self.vectors_total.saturating_add(points);
        self.vectors_built = self.vectors_built.saturating_add(points);
        self.vectors_final |= vectors_final;
        self.vector_counts(self.vectors_built)
    }

    pub(super) fn upserting_counts(&self) -> StageCounts {
        self.vector_counts(self.vectors_upserted)
    }

    pub(super) fn upserted(&mut self, points: u64) -> StageCounts {
        self.vectors_upserted = self.vectors_upserted.saturating_add(points);
        self.upserting_counts()
    }

    fn documents_total(&self) -> Option<u64> {
        self.documents_final.then_some(self.documents_total)
    }

    fn chunks_total(&self) -> Option<u64> {
        self.chunks_final.then_some(self.chunks_total)
    }

    fn vector_counts(&self, done: u64) -> StageCounts {
        stage_counts(
            self.documents_total(),
            self.documents_prepared,
            self.documents_total(),
            self.documents_prepared,
            self.vectors_final.then_some(self.vectors_total),
            done,
        )
    }
}

pub(super) fn stage_counts(
    items_total: Option<u64>,
    items_done: u64,
    documents_total: Option<u64>,
    documents_done: u64,
    chunks_total: Option<u64>,
    chunks_done: u64,
) -> StageCounts {
    StageCounts {
        items_total,
        items_done,
        documents_total,
        documents_done,
        chunks_total,
        chunks_done,
        bytes_total: None,
        bytes_done: 0,
    }
}

fn normalize_phase_counts(
    prior: &mut Vec<(PipelinePhase, StageCounts)>,
    phase: PipelinePhase,
    mut counts: StageCounts,
) -> StageCounts {
    if let Some((_, previous)) = prior.iter().find(|(stored, _)| *stored == phase) {
        counts.items_total = max_total(previous.items_total, counts.items_total);
        counts.documents_total = max_total(previous.documents_total, counts.documents_total);
        counts.chunks_total = max_total(previous.chunks_total, counts.chunks_total);
        counts.items_done = counts.items_done.max(previous.items_done);
        counts.documents_done = counts.documents_done.max(previous.documents_done);
        counts.chunks_done = counts.chunks_done.max(previous.chunks_done);
    }
    counts.items_done = clamp_done(counts.items_done, counts.items_total);
    counts.documents_done = clamp_done(counts.documents_done, counts.documents_total);
    counts.chunks_done = clamp_done(counts.chunks_done, counts.chunks_total);
    if let Some((_, previous)) = prior.iter_mut().find(|(stored, _)| *stored == phase) {
        *previous = counts.clone();
    } else {
        prior.push((phase, counts.clone()));
    }
    counts
}

fn max_total(previous: Option<u64>, current: Option<u64>) -> Option<u64> {
    previous.or(current)
}

fn clamp_done(done: u64, total: Option<u64>) -> u64 {
    total.map_or(done, |total| done.min(total))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
