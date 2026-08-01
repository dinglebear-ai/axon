//! Durable progress coordination for the web source pipeline.
//!
//! Web acquisition has adapter-owned fine-grained callbacks, while the
//! service owns generation-wide downstream counters and job persistence. The
//! coordinator writes one canonical snapshot to the durable job row and emits
//! the same snapshot to public progress events. Persistence is observational:
//! failures are logged and never replace the source pipeline result.

use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::{AcquisitionProgress, AcquisitionProgressSink};
use axon_api::source::{
    JobHeartbeat, JobStatusUpdate, LifecycleStatus, PipelinePhase, SourceId, StageCounts,
};
use axon_embedding::reservation::ProviderReservation;
use axon_jobs::boundary::JobStore;

use super::run::timestamp;
use crate::source::events::SourceEventEmitter;

use super::WebSourceIndexInput;

pub(super) struct WebProgressCoordinator {
    jobs: Option<Arc<dyn JobStore>>,
    job_id: axon_api::source::JobId,
    source_id: SourceId,
    attempt: u32,
}

impl WebProgressCoordinator {
    pub(super) fn new(input: &WebSourceIndexInput, source_id: SourceId) -> Self {
        Self {
            jobs: input.event_store.clone(),
            job_id: input.job_id,
            source_id,
            attempt: input.attempt,
        }
    }

    pub(super) async fn report(
        &self,
        events: &SourceEventEmitter,
        phase: PipelinePhase,
        counts: StageCounts,
        message: &'static str,
    ) {
        self.persist(phase, counts.clone(), message).await;
        events
            .running_with_counts(phase, message, Some(counts))
            .await;
    }

    pub(super) async fn checkpoint(
        &self,
        events: &SourceEventEmitter,
        phase: PipelinePhase,
        counts: StageCounts,
        message: &'static str,
    ) {
        self.report(events, phase, counts, message).await;
    }

    pub(super) async fn heartbeat(
        &self,
        phase: PipelinePhase,
        counts: StageCounts,
        reservation: &ProviderReservation,
    ) {
        let Some(jobs) = &self.jobs else {
            return;
        };
        let heartbeat = JobHeartbeat {
            job_id: self.job_id,
            attempt: self.attempt,
            worker_id: Some("web-source-pipeline".to_string()),
            phase,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: timestamp(),
            sequence: 0,
            last_progress_at: Some(timestamp()),
            last_event_sequence: None,
            counts: Some(counts),
            provider_reservations: vec![reservation.snapshot()],
        };
        if let Err(error) = jobs.heartbeat(heartbeat).await {
            tracing::warn!(
                job_id = %self.job_id.0,
                phase = ?phase,
                error = %error,
                "failed to persist web provider reservation heartbeat"
            );
        }
    }

    async fn persist(&self, phase: PipelinePhase, counts: StageCounts, message: &'static str) {
        let Some(jobs) = &self.jobs else {
            return;
        };
        if let Err(error) = jobs
            .update_status(JobStatusUpdate {
                job_id: self.job_id,
                source_id: Some(self.source_id.clone()),
                status: LifecycleStatus::Running,
                phase,
                stage_id: None,
                counts: Some(counts),
                current: None,
                message: Some(message.to_string()),
                error: None,
            })
            .await
        {
            tracing::warn!(
                job_id = %self.job_id.0,
                phase = ?phase,
                error = %error,
                "failed to persist web source progress"
            );
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct WebPipelineProgress {
    changed_total: u64,
    items_done: u64,
    acquired_documents: u64,
    normalized_documents: u64,
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

impl WebPipelineProgress {
    pub(super) fn new(changed_total: u64) -> Self {
        Self {
            changed_total,
            ..Self::default()
        }
    }

    pub(super) fn fetch_start(&self) -> StageCounts {
        self.fetch_counts(self.items_done, self.acquired_documents)
    }

    pub(super) fn acquisition_sink<'a>(
        &'a self,
        coordinator: &'a WebProgressCoordinator,
        events: &'a SourceEventEmitter,
        batch_total: u64,
    ) -> WebAcquisitionProgressSink<'a> {
        WebAcquisitionProgressSink {
            coordinator,
            events,
            item_offset: self.items_done,
            document_offset: self.acquired_documents,
            changed_total: self.changed_total,
            batch_total,
        }
    }

    pub(super) fn acquired(&mut self, items: u64, documents: u64) -> StageCounts {
        self.items_done = self
            .items_done
            .saturating_add(items)
            .min(self.changed_total);
        self.acquired_documents = self
            .acquired_documents
            .saturating_add(documents)
            .min(self.items_done);
        self.fetch_counts(self.items_done, self.acquired_documents)
    }

    pub(super) fn enriching_counts(&self) -> StageCounts {
        self.fetch_counts(self.items_done, self.acquired_documents)
    }

    pub(super) fn normalizing_counts(&self) -> StageCounts {
        stage_counts(
            Some(self.changed_total),
            self.items_done,
            self.documents_total(),
            self.normalized_documents,
            None,
            0,
        )
    }

    pub(super) fn normalized(&mut self, documents: u64, final_batch: bool) -> StageCounts {
        self.normalized_documents = self
            .normalized_documents
            .saturating_add(documents)
            .min(self.acquired_documents);
        self.documents_final |= final_batch;
        stage_counts(
            Some(self.changed_total),
            self.items_done,
            self.documents_total(),
            self.normalized_documents,
            None,
            0,
        )
    }

    pub(super) fn preparing_counts(&self) -> StageCounts {
        stage_counts(
            Some(self.changed_total),
            self.items_done,
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
        final_batch: bool,
    ) -> StageCounts {
        self.documents_prepared = self
            .documents_prepared
            .saturating_add(documents)
            .min(self.normalized_documents);
        self.chunks_total = self.chunks_total.saturating_add(chunks);
        self.chunks_final |= final_batch;
        if self.chunks_final {
            self.chunks_batched = self.chunks_batched.min(self.chunks_total);
            self.chunks_embedded = self.chunks_embedded.min(self.chunks_total);
        }
        self.preparing_counts()
    }

    pub(super) fn batched(&mut self, chunks: u64) -> StageCounts {
        self.chunks_batched = self.chunks_batched.saturating_add(chunks);
        if self.chunks_final {
            self.chunks_batched = self.chunks_batched.min(self.chunks_total);
        }
        self.chunk_counts(self.chunks_batched)
    }

    pub(super) fn embedding_counts(&self) -> StageCounts {
        self.chunk_counts(self.chunks_embedded)
    }

    pub(super) fn embedded(&mut self, chunks: u64) -> StageCounts {
        self.chunks_embedded = self.chunks_embedded.saturating_add(chunks);
        if self.chunks_final {
            self.chunks_embedded = self.chunks_embedded.min(self.chunks_total);
        }
        self.embedding_counts()
    }

    pub(super) fn vectorized(&mut self, points: u64, final_batch: bool) -> StageCounts {
        self.vectors_total = self.vectors_total.saturating_add(points);
        self.vectors_built = self.vectors_built.saturating_add(points);
        self.vectors_final |= final_batch;
        if self.vectors_final {
            self.vectors_built = self.vectors_built.min(self.vectors_total);
            self.vectors_upserted = self.vectors_upserted.min(self.vectors_total);
        }
        self.vector_counts(self.vectors_built)
    }

    pub(super) fn upserting_counts(&self) -> StageCounts {
        self.vector_counts(self.vectors_upserted)
    }

    pub(super) fn upserted(&mut self, points: u64) -> StageCounts {
        self.vectors_upserted = self.vectors_upserted.saturating_add(points);
        if self.vectors_final {
            self.vectors_upserted = self.vectors_upserted.min(self.vectors_total);
        }
        self.upserting_counts()
    }

    fn documents_total(&self) -> Option<u64> {
        self.documents_final.then_some(self.normalized_documents)
    }

    fn chunks_total(&self) -> Option<u64> {
        self.chunks_final.then_some(self.chunks_total)
    }

    fn fetch_counts(&self, items_done: u64, documents_done: u64) -> StageCounts {
        stage_counts(
            Some(self.changed_total),
            items_done,
            Some(self.changed_total),
            documents_done,
            None,
            0,
        )
    }

    fn chunk_counts(&self, done: u64) -> StageCounts {
        stage_counts(
            Some(self.changed_total),
            self.items_done,
            self.documents_total(),
            self.documents_prepared,
            self.chunks_total(),
            done,
        )
    }

    fn vector_counts(&self, done: u64) -> StageCounts {
        stage_counts(
            Some(self.changed_total),
            self.items_done,
            self.documents_total(),
            self.documents_prepared,
            self.vectors_final.then_some(self.vectors_total),
            done,
        )
    }
}

pub(super) struct WebAcquisitionProgressSink<'a> {
    coordinator: &'a WebProgressCoordinator,
    events: &'a SourceEventEmitter,
    item_offset: u64,
    document_offset: u64,
    changed_total: u64,
    batch_total: u64,
}

#[async_trait]
impl AcquisitionProgressSink for WebAcquisitionProgressSink<'_> {
    async fn report(&self, progress: AcquisitionProgress) {
        let batch_items = progress.items_done.min(self.batch_total);
        let batch_documents = progress.documents_done.min(batch_items);
        let counts = stage_counts(
            Some(self.changed_total),
            self.item_offset.saturating_add(batch_items),
            Some(self.changed_total),
            self.document_offset.saturating_add(batch_documents),
            None,
            0,
        );
        self.coordinator
            .report(
                self.events,
                PipelinePhase::Fetching,
                counts,
                "fetching changed web source items",
            )
            .await;
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

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
