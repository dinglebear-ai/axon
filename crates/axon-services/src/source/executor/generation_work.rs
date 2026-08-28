//! Neutral work exchanged between generation preparation and vectorization.

use axon_api::source::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::vectorize::batching::chunk_batches;
use crate::source::output::SourceOutput;

const CHANNEL_CAPACITY: usize = 2;
const BYTE_BUDGET_KIB: u32 = 1_048_576;
const KIB: usize = 1024;

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

    fn estimated_bytes(&self) -> anyhow::Result<usize> {
        let serializable = (
            &self.acquisition_artifacts,
            &self.enrichment_artifacts,
            &self.archive_items,
            &self.artifact_candidates,
            &self.warnings,
            &self.reused_item_keys,
            &self.refreshed_manifest_items,
            &self.clean_output.artifacts,
            &self.clean_output.inline,
        );
        Ok(serde_json::to_vec(&serializable)?.len())
    }
}

/// One lossless, prepared acquisition wave. The sender may split this into
/// smaller envelopes, but must retain every document and side effect exactly
/// once and preserve FIFO sequence order.
pub(super) struct PreparedGenerationBatch {
    pub(super) sequence: u64,
    pub(super) prepared: Vec<PreparedDocument>,
    pub(super) side_effects: PreparedBatchSideEffects,
    pub(super) is_final: bool,
}

impl PreparedGenerationBatch {
    pub(super) fn chunk_count(&self) -> usize {
        self.prepared
            .iter()
            .map(|document| document.chunks.len())
            .sum()
    }
}

pub(super) struct PreparedWorkEnvelope {
    pub(super) sequence: u64,
    pub(super) prepared: Vec<PreparedDocument>,
    pub(super) side_effects: PreparedBatchSideEffects,
    pub(super) is_final: bool,
    pub(super) estimated_bytes: usize,
    _chunk_permit: OwnedSemaphorePermit,
    _byte_permit: OwnedSemaphorePermit,
}

#[derive(Clone)]
pub(super) struct PreparedBatchSender {
    sender: mpsc::Sender<PreparedWorkEnvelope>,
    chunk_permits: Arc<Semaphore>,
    byte_permits: Arc<Semaphore>,
    pool_size: usize,
    sequence: Arc<AtomicU64>,
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
            byte_permits: Arc::new(Semaphore::new(BYTE_BUDGET_KIB as usize)),
            pool_size,
            sequence: Arc::new(AtomicU64::new(0)),
        },
        PreparedBatchReceiver { receiver },
    ))
}

impl PreparedBatchSender {
    pub(super) async fn send(
        &self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.send_final(prepared, side_effects, false, cancel).await
    }

    pub(super) async fn send_final(
        &self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        is_final: bool,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        let batch = PreparedGenerationBatch {
            sequence: self.sequence.load(Ordering::Relaxed),
            prepared,
            side_effects,
            is_final,
        };
        let _chunk_count = batch.chunk_count();
        let PreparedGenerationBatch {
            sequence: _batch_sequence,
            prepared,
            side_effects,
            is_final,
        } = batch;
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
        &self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        is_final: bool,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        let charged_chunks = prepared
            .iter()
            .map(|document| document.chunks.len().max(1))
            .sum::<usize>();
        anyhow::ensure!(
            charged_chunks <= self.pool_size,
            "prepared pool exceeds chunk limit"
        );
        let prepared_bytes = serde_json::to_vec(&prepared)?.len();
        let estimated_bytes = prepared_bytes
            .checked_add(side_effects.estimated_bytes()?)
            .ok_or_else(|| anyhow::anyhow!("prepared work byte size overflow"))?;
        anyhow::ensure!(
            estimated_bytes <= 1024 * 1024 * 1024,
            "prepared item exceeds 1 GiB"
        );
        let byte_units = estimated_bytes.max(1).div_ceil(KIB);
        let chunk_units = u32::try_from(charged_chunks.max(1))?;
        let byte_units = u32::try_from(byte_units)?;
        let chunk_permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            permit = Arc::clone(&self.chunk_permits).acquire_many_owned(chunk_units) => permit?,
        };
        let byte_permit = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            permit = Arc::clone(&self.byte_permits).acquire_many_owned(byte_units) => permit?,
        };
        let envelope = PreparedWorkEnvelope {
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            prepared,
            side_effects,
            is_final,
            estimated_bytes,
            _chunk_permit: chunk_permit,
            _byte_permit: byte_permit,
        };
        tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("prepared work send canceled"),
            result = self.sender.send(envelope) => result.map_err(|_| anyhow::anyhow!("prepared work receiver closed")),
        }
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
