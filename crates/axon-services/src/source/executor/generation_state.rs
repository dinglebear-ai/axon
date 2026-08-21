//! Aggregation state for one created source generation.

use std::collections::{BTreeSet, HashSet};

use axon_api::source::*;

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

pub(super) struct ProcessedBatch {
    pub(super) vectorized: vectorize::VectorizeResult,
    pub(super) acquisition_artifacts: Vec<ArtifactRef>,
    pub(super) enrichment_artifacts: Vec<ArtifactRef>,
    pub(super) clean_output: SourceOutput,
    pub(super) archive_items: Vec<AcquiredSourceItem>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) refreshed_manifest_items: Vec<ManifestItem>,
}

#[derive(Default)]
pub(super) struct GenerationAccumulator {
    vectorized: vectorize::VectorizeResult,
    document_ids: HashSet<DocumentId>,
    artifacts: Vec<ArtifactRef>,
    output: SourceOutput,
    archive_items: Vec<AcquiredSourceItem>,
    warnings: Vec<SourceWarning>,
    reused_item_keys: BTreeSet<SourceItemKey>,
    refreshed_manifest_items: Vec<ManifestItem>,
}

pub(super) struct FinalizedGeneration {
    pub(super) diff: SourceManifestDiff,
    pub(super) vectorized: vectorize::VectorizeResult,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) inline: Option<InlineSourceResult>,
}

impl GenerationAccumulator {
    pub(super) fn absorb(&mut self, cleanup: &mut ArtifactCleanupGuard, batch: ProcessedBatch) {
        cleanup.track(&batch.acquisition_artifacts);
        cleanup.track(&batch.enrichment_artifacts);
        cleanup.track(&batch.clean_output.artifacts);
        self.artifacts.extend(batch.acquisition_artifacts);
        self.artifacts.extend(batch.enrichment_artifacts);
        self.archive_items.extend(batch.archive_items);
        self.warnings.extend(batch.warnings);
        self.reused_item_keys.extend(batch.reused_item_keys);
        self.refreshed_manifest_items
            .extend(batch.refreshed_manifest_items);
        self.output.merge(batch.clean_output);

        // Per-batch statuses have already been durably written. Retain only
        // document identities for generation-wide deduplication; publication
        // promotes the durable rows with one ledger-side update instead of
        // carrying every full status object until the end of a large crawl.
        let vectorized = batch.vectorized;
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
        self.vectorized.warnings.splice(0..0, self.warnings);
        let archive =
            output::store_adapter_archive(runtime, input.adapter, &input.plan, &self.archive_items)
                .await?;
        cleanup.track(&archive.artifacts);
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
            inline: self.output.inline,
        })
    }
}
