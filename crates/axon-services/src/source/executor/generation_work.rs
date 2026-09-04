//! Neutral work exchanged between generation preparation and vectorization.

use axon_api::source::*;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::vectorize::batching::chunk_batches;
use crate::source::output::SourceOutput;

const CHANNEL_CAPACITY: usize = 2;
#[cfg(test)]
const BYTE_BUDGET_KIB: u32 = 131_072;
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
        serialized_size(&serializable)
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

pub(super) struct PreparedBatchSender {
    sender: mpsc::Sender<PreparedWorkEnvelope>,
    chunk_permits: Arc<Semaphore>,
    byte_permits: Arc<Semaphore>,
    byte_budget: usize,
    pool_size: usize,
    sequence: u64,
}

pub(super) struct PreparedBatchReceiver {
    receiver: mpsc::Receiver<PreparedWorkEnvelope>,
}

#[cfg(test)]
pub(super) fn prepared_work_channel(
    pool_size: usize,
) -> anyhow::Result<(PreparedBatchSender, PreparedBatchReceiver)> {
    prepared_work_channel_with_byte_budget(pool_size, BYTE_BUDGET_KIB as usize * KIB)
}

pub(super) fn prepared_work_channel_with_byte_budget(
    pool_size: usize,
    byte_budget: usize,
) -> anyhow::Result<(PreparedBatchSender, PreparedBatchReceiver)> {
    anyhow::ensure!(pool_size > 0, "embedding pool size must be positive");
    anyhow::ensure!(
        byte_budget > 0,
        "prepared work byte budget must be positive"
    );
    let byte_budget_kib = byte_budget.div_ceil(KIB).min(u32::MAX as usize);
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
            byte_permits: Arc::new(Semaphore::new(byte_budget_kib)),
            byte_budget,
            pool_size,
            sequence: 0,
        },
        PreparedBatchReceiver { receiver },
    ))
}

impl PreparedBatchSender {
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
        let batch = PreparedGenerationBatch {
            sequence: self.sequence,
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
        &mut self,
        prepared: Vec<PreparedDocument>,
        side_effects: PreparedBatchSideEffects,
        is_final: bool,
        cancel: &CancellationToken,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(!cancel.is_cancelled(), "prepared work send canceled");
        let charged_chunks = prepared
            .iter()
            .map(|document| document.chunks.len().max(1))
            .sum::<usize>();
        anyhow::ensure!(
            charged_chunks <= self.pool_size,
            "prepared pool exceeds chunk limit"
        );
        let prepared_bytes = serialized_size(&prepared)?;
        let estimated_bytes = prepared_bytes
            .checked_add(side_effects.estimated_bytes()?)
            .ok_or_else(|| anyhow::anyhow!("prepared work byte size overflow"))?;
        anyhow::ensure!(
            estimated_bytes <= self.byte_budget,
            "prepared item exceeds configured byte budget"
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
            sequence: self.sequence,
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
        }?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }
}

#[derive(Default)]
struct SizeWriter(usize);

impl std::io::Write for SizeWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.checked_add(buffer.len()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::OutOfMemory, "serialized size overflow")
        })?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialized_size(value: &impl serde::Serialize) -> anyhow::Result<usize> {
    let mut writer = SizeWriter::default();
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.0)
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
