//! Neutral work exchanged between generation preparation and vectorization.

use axon_api::source::*;
use std::sync::{Arc, LazyLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::vectorize::batching::{charged_chunk_count, chunk_batches};
use crate::source::output::SourceOutput;

const CHANNEL_CAPACITY: usize = 2;
const BYTE_BUDGET_KIB: u32 = 1_048_576;
const PER_JOB_BYTE_BUDGET_KIB: u32 = 262_144;
const KIB: usize = 1024;

static PROCESS_BYTE_PERMITS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(BYTE_BUDGET_KIB as usize)));

/// Side effects are cleanup-owned before this value is constructed. Moving it
/// into the accumulator transfers only finalization/accounting ownership.
pub(super) struct PreparedBatchSideEffects {
    pub(super) acquisition_artifacts: Vec<ArtifactRef>,
    pub(super) enrichment_artifacts: Vec<ArtifactRef>,
    pub(super) clean_output: SourceOutput,
    pub(super) archive_items: Vec<AcquiredSourceItem>,
    pub(super) artifact_candidates: Vec<ArtifactCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) refreshed_manifest_items: Vec<ManifestItem>,
}

impl PreparedBatchSideEffects {
    pub(super) fn empty() -> Self {
        Self {
            acquisition_artifacts: Vec::new(),
            enrichment_artifacts: Vec::new(),
            clean_output: SourceOutput::default(),
            archive_items: Vec::new(),
            artifact_candidates: Vec::new(),
            warnings: Vec::new(),
            reused_item_keys: Vec::new(),
            refreshed_manifest_items: Vec::new(),
        }
    }

    fn estimated_resident_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            .saturating_add(vector_resident_bytes(&self.acquisition_artifacts))
            .saturating_add(vector_resident_bytes(&self.enrichment_artifacts))
            .saturating_add(vector_resident_bytes(&self.archive_items))
            .saturating_add(vector_resident_bytes(&self.artifact_candidates))
            .saturating_add(vector_resident_bytes(&self.warnings))
            .saturating_add(vector_resident_bytes(&self.reused_item_keys))
            .saturating_add(vector_resident_bytes(&self.refreshed_manifest_items))
            .saturating_add(vector_resident_bytes(&self.clean_output.artifacts))
            .saturating_add(
                self.clean_output
                    .inline
                    .as_ref()
                    .map_or(0, |_| std::mem::size_of::<InlineSourceResult>() + 256),
            )
    }
}

fn vector_resident_bytes<T>(values: &Vec<T>) -> usize {
    values
        .capacity()
        .saturating_mul(std::mem::size_of::<T>())
        .saturating_add(values.len().saturating_mul(256))
}

fn prepared_resident_bytes(documents: &[PreparedDocument]) -> usize {
    documents
        .iter()
        .fold(std::mem::size_of_val(documents), |total, document| {
            document.chunks.iter().fold(total, |total, chunk| {
                total
                    .saturating_add(std::mem::size_of::<PreparedChunk>())
                    .saturating_add(chunk.content.capacity())
                    .saturating_add(chunk.chunk_key.capacity())
                    .saturating_add(chunk.content_hash.capacity())
                    .saturating_add(chunk.embedding_text.as_ref().map_or(0, String::capacity))
            })
        })
}

pub(super) struct PreparedWorkEnvelope {
    pub(super) sequence: u64,
    pub(super) prepared: Vec<PreparedDocument>,
    pub(super) side_effects: PreparedBatchSideEffects,
    pub(super) is_final: bool,
    pub(super) estimated_bytes: usize,
    _chunk_permit: OwnedSemaphorePermit,
    _job_byte_permit: OwnedSemaphorePermit,
    _process_byte_permit: OwnedSemaphorePermit,
}

pub(super) struct PreparedBatchSender {
    sender: mpsc::Sender<PreparedWorkEnvelope>,
    chunk_permits: Arc<Semaphore>,
    job_byte_permits: Arc<Semaphore>,
    process_byte_permits: Arc<Semaphore>,
    pool_size: usize,
    sequence: u64,
}

pub(super) struct PreparedBatchReceiver {
    receiver: mpsc::Receiver<PreparedWorkEnvelope>,
}

pub(super) fn prepared_work_channel(
    pool_size: usize,
) -> anyhow::Result<(PreparedBatchSender, PreparedBatchReceiver)> {
    anyhow::ensure!(pool_size > 0, "embedding pool size must be positive");
    let chunk_capacity = pool_size
        .checked_mul(3)
        .ok_or_else(|| anyhow::anyhow!("embedding pool size overflows chunk capacity"))?;
    anyhow::ensure!(
        u32::try_from(chunk_capacity).is_ok(),
        "embedding chunk capacity exceeds semaphore limit"
    );
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    Ok((
        PreparedBatchSender {
            sender,
            chunk_permits: Arc::new(Semaphore::new(chunk_capacity)),
            job_byte_permits: Arc::new(Semaphore::new(PER_JOB_BYTE_BUDGET_KIB as usize)),
            process_byte_permits: Arc::clone(&PROCESS_BYTE_PERMITS),
            pool_size,
            sequence: 0,
        },
        PreparedBatchReceiver { receiver },
    ))
}

impl PreparedBatchSender {
    #[cfg(test)]
    pub(super) async fn send(
        &mut self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.send_final(prepared, side_effects, false, cancel).await
    }

    pub(super) async fn send_final(
        &mut self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        is_final: bool,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        let mut pools = chunk_batches(prepared, self.pool_size)
            .into_iter()
            .peekable();
        if pools.peek().is_none() {
            return self
                .send_pool(Vec::new(), side_effects, is_final, cancel)
                .await;
        }
        let mut side_effects = Some(side_effects);
        while let Some(pool) = pools.next() {
            let pool_is_final = is_final && pools.peek().is_none();
            self.send_pool(
                pool,
                side_effects
                    .take()
                    .unwrap_or_else(PreparedBatchSideEffects::empty),
                pool_is_final,
                cancel,
            )
            .await?;
        }
        Ok(())
    }

    async fn send_pool(
        &mut self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        is_final: bool,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(!cancel.is_cancelled(), "prepared work send canceled");
        let charged_chunks = charged_chunk_count(&prepared);
        anyhow::ensure!(
            charged_chunks <= self.pool_size,
            "prepared pool exceeds chunk limit"
        );
        let prepared_bytes = prepared_resident_bytes(&prepared);
        let estimated_bytes = prepared_bytes
            .checked_add(side_effects.estimated_resident_bytes())
            .ok_or_else(|| anyhow::anyhow!("prepared work byte size overflow"))?;
        anyhow::ensure!(
            estimated_bytes <= PER_JOB_BYTE_BUDGET_KIB as usize * KIB,
            "prepared item exceeds the 256 MiB per-job ceiling"
        );
        let byte_units = estimated_bytes.max(1).div_ceil(KIB);
        let chunk_units = u32::try_from(charged_chunks.max(1))?;
        let byte_units = u32::try_from(byte_units)?;
        let chunk_permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            permit = Arc::clone(&self.chunk_permits).acquire_many_owned(chunk_units) => permit?,
        };
        let job_byte_permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            permit = Arc::clone(&self.job_byte_permits).acquire_many_owned(byte_units) => permit?,
        };
        let process_byte_permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            permit = Arc::clone(&self.process_byte_permits).acquire_many_owned(byte_units) => permit?,
        };
        let envelope = PreparedWorkEnvelope {
            sequence: self.sequence,
            prepared,
            side_effects,
            is_final,
            estimated_bytes,
            _chunk_permit: chunk_permit,
            _job_byte_permit: job_byte_permit,
            _process_byte_permit: process_byte_permit,
        };
        tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            result = self.sender.send(envelope) => result.map_err(|_| anyhow::anyhow!("prepared work receiver closed")),
        }?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }
}

impl PreparedBatchReceiver {
    pub(super) async fn recv(&mut self) -> Option<PreparedWorkEnvelope> {
        self.receiver.recv().await
    }

    pub(super) fn is_channel_closed(&self) -> bool {
        self.receiver.is_closed()
    }
}

#[cfg(test)]
#[path = "generation_work_tests.rs"]
mod tests;
