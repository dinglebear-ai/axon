//! Structured `SourceProgressEvent` projection shared by the web and non-web
//! source pipelines: phase completions carry generation + stage counts, item
//! warnings/errors carry the failing item, and terminal failures carry the
//! pipeline error.

use axon_api::source::*;

use super::events::{SourceEventDetails, SourceEventEmitter};

pub(crate) async fn pipeline_failed(emitter: &SourceEventEmitter, error: &anyhow::Error) {
    emitter
        .failed_with_error(
            PipelinePhase::Complete,
            "source pipeline failed",
            ApiError::new(
                "source.index_failed",
                ErrorStage::Internal,
                error.to_string(),
            ),
        )
        .await;
}

pub(crate) async fn discovered(emitter: &SourceEventEmitter, manifest: &SourceManifest) {
    emitter
        .completed_with(
            PipelinePhase::Discovering,
            "discovered source items",
            SourceEventDetails {
                generation: Some(manifest.generation.clone()),
                counts: Some(item_counts(manifest.items.len() as u64)),
                ..SourceEventDetails::default()
            },
        )
        .await;
}

pub(crate) async fn diffed(emitter: &SourceEventEmitter, diff: &SourceManifestDiff) {
    emitter
        .completed_with(
            PipelinePhase::Diffing,
            "diffed source manifest",
            SourceEventDetails {
                generation: Some(diff.next_generation.clone()),
                counts: Some(diff_stage_counts(diff)),
                ..SourceEventDetails::default()
            },
        )
        .await;
    for failure in diff.skipped.iter().chain(&diff.failed) {
        emitter
            .item_error(
                PipelinePhase::Diffing,
                failure.error.clone(),
                Some(diff.next_generation.clone()),
            )
            .await;
    }
}

pub(crate) async fn acquired(emitter: &SourceEventEmitter, acquisition: &SourceAcquisition) {
    emitter
        .completed_with(
            PipelinePhase::Fetching,
            "acquired changed source items",
            SourceEventDetails {
                generation: Some(acquisition.generation.clone()),
                counts: Some(acquisition.header.counts.clone()),
                ..SourceEventDetails::default()
            },
        )
        .await;
}

pub(crate) async fn normalized(
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    header: &StageResultHeader,
) {
    emitter
        .completed_with(
            PipelinePhase::Normalizing,
            "normalized source documents",
            SourceEventDetails {
                generation: Some(generation.clone()),
                counts: Some(header.counts.clone()),
                ..SourceEventDetails::default()
            },
        )
        .await;
}

pub(crate) async fn published(
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    manifest_items: u64,
    warnings: &[SourceWarning],
    documents_prepared: u64,
    chunks_prepared: u64,
) {
    for warning in warnings {
        emitter
            .warning(
                PipelinePhase::Publishing,
                warning.clone(),
                Some(generation.clone()),
            )
            .await;
    }
    emitter
        .completed_with(
            PipelinePhase::Publishing,
            "published source generation",
            SourceEventDetails {
                generation: Some(generation.clone()),
                counts: Some(StageCounts {
                    items_total: Some(manifest_items),
                    items_done: manifest_items,
                    documents_total: Some(documents_prepared),
                    documents_done: documents_prepared,
                    chunks_total: Some(chunks_prepared),
                    chunks_done: chunks_prepared,
                    bytes_total: None,
                    bytes_done: 0,
                }),
                ..SourceEventDetails::default()
            },
        )
        .await;
}

fn item_counts(items: u64) -> StageCounts {
    StageCounts {
        items_total: Some(items),
        items_done: items,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}

fn diff_stage_counts(diff: &SourceManifestDiff) -> StageCounts {
    let items = diff
        .counts
        .added
        .saturating_add(diff.counts.modified)
        .saturating_add(diff.counts.removed)
        .saturating_add(diff.counts.unchanged)
        .saturating_add(diff.counts.skipped)
        .saturating_add(diff.counts.failed);
    item_counts(items)
}
