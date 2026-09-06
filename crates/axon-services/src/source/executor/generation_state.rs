//! Aggregation state for one created source generation.

use std::collections::{BTreeSet, HashSet};

use anyhow::Context as _;
use axon_api::source::*;

use super::generation_spool::{GenerationSpool, SideEffectsSpoolRecord};
use super::generation_work::PreparedBatchSideEffects;
use super::progress::PipelineProgress;
use super::{SourcePipelineInput, reuse, vectorize};
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::ArtifactCleanupGuard;
use crate::source::output::{self, SourceOutput};

#[derive(Default)]
pub(super) struct GenerationStageProgress {
    pub(super) pipeline: PipelineProgress,
    pub(super) acquired_items: u64,
    pub(super) acquired_documents: u64,
    pub(super) enriched_items: u64,
    pub(super) normalized_documents: u64,
}

#[derive(Default)]
pub(super) struct GenerationAccumulator {
    vectorized: vectorize::VectorizeResult,
    document_ids: HashSet<DocumentId>,
    artifacts: Vec<ArtifactRef>,
    output: SourceOutput,
    archive_items: Vec<AcquiredSourceItem>,
    artifact_candidates: Vec<ArtifactCandidate>,
    warnings: Vec<SourceWarning>,
    reused_item_keys: BTreeSet<SourceItemKey>,
    refreshed_manifest_items: Vec<ManifestItem>,
    spool: Option<GenerationSpool>,
    spool_sequence: u64,
}

pub(super) struct FinalizedGeneration {
    pub(super) diff: SourceManifestDiff,
    pub(super) vectorized: vectorize::VectorizeResult,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) artifact_candidates: Vec<ArtifactCandidate>,
    pub(super) inline: Option<InlineSourceResult>,
}

impl GenerationAccumulator {
    pub(super) fn new(generation: &SourceGenerationId) -> Self {
        let spool = match GenerationSpool::temporary(&generation.0) {
            Ok(spool) => Some(spool),
            Err(error) => {
                tracing::warn!(error = %error, "generation spool unavailable; retaining side effects in memory");
                None
            }
        };
        Self {
            spool,
            ..Self::default()
        }
    }

    pub(super) fn absorb_pretracked_side_effects(
        &mut self,
        batch: PreparedBatchSideEffects,
    ) -> anyhow::Result<()> {
        self.artifacts.extend(batch.acquisition_artifacts);
        self.artifacts.extend(batch.enrichment_artifacts);
        self.output.merge(batch.clean_output);
        let record = SideEffectsSpoolRecord {
            archive_items: batch.archive_items,
            artifact_candidates: batch.artifact_candidates,
            warnings: batch.warnings,
            reused_item_keys: batch.reused_item_keys,
            refreshed_manifest_items: batch.refreshed_manifest_items,
        };
        let key = format!("side-effects:{}", self.spool_sequence);
        self.spool_sequence = self.spool_sequence.saturating_add(1);
        let append_error = self
            .spool
            .as_mut()
            .and_then(|spool| spool.append(&key, &record).err());
        if let Some(error) = append_error {
            tracing::warn!(error = %error, "generation spool append failed; retaining side effects in memory");
            if let Some(spool) = self.spool.take() {
                spool
                    .replay_each::<SideEffectsSpoolRecord>(|prior_key, prior| {
                        // A flush error can be ambiguous: the current record
                        // may already be readable. Absorb it exactly once via
                        // the authoritative in-memory value below.
                        if prior_key != key {
                            self.absorb_side_effect_record(prior);
                        }
                        Ok(())
                    })
                    .context("generation spool replay failed after append error")?;
            }
            self.absorb_side_effect_record(record);
        } else if self.spool.is_none() {
            self.absorb_side_effect_record(record);
        }
        Ok(())
    }

    fn absorb_side_effect_record(&mut self, record: SideEffectsSpoolRecord) {
        self.archive_items.extend(record.archive_items);
        self.artifact_candidates.extend(record.artifact_candidates);
        self.warnings.extend(record.warnings);
        self.reused_item_keys.extend(record.reused_item_keys);
        self.refreshed_manifest_items
            .extend(record.refreshed_manifest_items);
    }

    pub(super) fn absorb_vectorized(&mut self, vectorized: vectorize::VectorizeResult) {
        // Per-pool statuses have already been durably written. Retain only
        // document identities for generation-wide deduplication.
        for status in &vectorized.document_statuses {
            if self.document_ids.insert(status.document_id.clone()) {
                self.vectorized.documents_prepared =
                    self.vectorized.documents_prepared.saturating_add(1);
            }
        }
        self.vectorized.chunks_prepared = self
            .vectorized
            .chunks_prepared
            .saturating_add(vectorized.chunks_prepared);
        self.vectorized.points_written = self
            .vectorized
            .points_written
            .saturating_add(vectorized.points_written);
        self.vectorized
            .graph_candidates
            .extend(vectorized.graph_candidates);
        self.vectorized.warnings.extend(vectorized.warnings);
    }

    pub(super) async fn finalize(
        mut self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
        cleanup: &mut ArtifactCleanupGuard,
        manifest: &mut SourceManifest,
        diff: SourceManifestDiff,
    ) -> anyhow::Result<FinalizedGeneration> {
        if let Some(spool) = self.spool.take() {
            spool.replay_each::<SideEffectsSpoolRecord>(|_, record| {
                self.absorb_side_effect_record(record);
                Ok(())
            })?;
        }
        self.vectorized.warnings.splice(0..0, self.warnings);
        let archive =
            output::store_adapter_archive(runtime, input.adapter, &input.plan, &self.archive_items)
                .await?;
        cleanup.track(&archive.artifacts).await?;
        self.output.merge(archive);
        self.artifacts.append(&mut self.output.artifacts);
        let diff = reuse::apply_reused_items(diff, &self.reused_item_keys);
        let refreshed = self
            .refreshed_manifest_items
            .into_iter()
            .map(|item| (item.source_item_key.clone(), item))
            .collect::<std::collections::BTreeMap<_, _>>();
        for item in &mut manifest.items {
            if let Some(replacement) = refreshed.get(&item.source_item_key) {
                *item = replacement.clone();
            }
        }
        output::record_artifacts_on_manifest(
            runtime.ledger.as_ref(),
            manifest,
            &diff,
            &self.output.artifact_index,
        )
        .await?;
        Ok(FinalizedGeneration {
            diff,
            vectorized: self.vectorized,
            artifacts: self.artifacts,
            artifact_candidates: self.artifact_candidates,
            inline: self.output.inline,
        })
    }
}
