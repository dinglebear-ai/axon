//! Web source acquisition + normalization (reuse-aware).
//!
//! Acquires the changed items for a web generation, resolves conditional-GET
//! reuse against the document cache, and normalizes the fetched items into
//! `SourceDocument`s. Extracted from `vectorize.rs` to keep that file under
//! the monolith cap; the vectorize stage consumes [`normalize_changed_documents`]
//! per diff batch.

use axon_adapters::{SourceAdapter, web};
use axon_api::source::*;

use super::WebSourceIndexInput;
use super::artifacts::WebArtifactIndex;
use super::normalize_batch::NormalizationBatch;
use crate::source::events::SourceEventEmitter;

use super::progress::{WebPipelineProgress, WebProgressCoordinator};
use super::run::WebAdapterRun;

pub(super) struct NormalizedWebDocuments {
    pub(super) documents: Vec<SourceDocument>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) inline: Option<InlineSourceResult>,
    pub(super) artifact_index: WebArtifactIndex,
}

pub(super) async fn normalize_changed_documents(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
    final_batch: bool,
) -> anyhow::Result<NormalizedWebDocuments> {
    let adapter = web::WebSourceAdapter::new(
        std::sync::Arc::clone(&input.fetch_provider),
        std::sync::Arc::clone(&input.render_provider),
    );
    let batch_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    let sink = progress.acquisition_sink(coordinator, events, batch_total);
    let mut acquisition = adapter
        .acquire_with_progress(&run.plan, diff, Some(&sink))
        .await?;
    let acquired_documents = acquisition.fetched_items.len() as u64;
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Fetching,
            progress.acquired(batch_total, acquired_documents),
            "fetched changed web source items",
        )
        .await;
    coordinator
        .report(
            events,
            PipelinePhase::Enriching,
            progress.enriching_counts(),
            "enriching web source items",
        )
        .await;
    coordinator
        .report(
            events,
            PipelinePhase::Normalizing,
            progress.normalizing_counts(),
            "normalizing web source documents",
        )
        .await;
    let batch = NormalizationBatch::collect(input, run, diff, &mut acquisition).await?;
    let normalized = batch.finish(input, run, diff, acquisition).await?;
    let completed_documents = normalized
        .documents
        .len()
        .saturating_add(normalized.reused_item_keys.len()) as u64;
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Normalizing,
            progress.normalized(completed_documents, final_batch),
            "normalized web source documents",
        )
        .await;
    Ok(normalized)
}

pub(super) async fn refetch_without_conditional(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    manifest_item: ManifestItem,
) -> anyhow::Result<AcquiredSourceItem> {
    let mut plan = run.plan.clone();
    plan.route
        .validated_options
        .values
        .insert("etag_conditional".to_string(), serde_json::json!(false));
    let adapter = web::WebSourceAdapter::new(
        std::sync::Arc::clone(&input.fetch_provider),
        std::sync::Arc::clone(&input.render_provider),
    );
    let reacquired = adapter
        .acquire(
            &plan,
            &SourceManifestDiff {
                header: diff.header.clone(),
                source_id: diff.source_id.clone(),
                previous_generation: diff.previous_generation.clone(),
                next_generation: diff.next_generation.clone(),
                added: Vec::new(),
                modified: vec![manifest_item.clone()],
                removed: Vec::new(),
                unchanged: Vec::new(),
                skipped: Vec::new(),
                failed: Vec::new(),
                counts: DiffCounts {
                    added: 0,
                    modified: 1,
                    removed: 0,
                    unchanged: 0,
                    skipped: 0,
                    failed: 0,
                },
            },
        )
        .await?;
    let mut reacquired_items = reacquired.fetched_items.into_iter();
    let reacquired = match reacquired_items.next() {
        Some(item) => item,
        None => {
            if let Some(warning) = reacquired.header.warnings.iter().find(|warning| {
                warning.code == "web.fetch.invalid_304_without_validator"
                    || warning.message.contains("304 Not Modified")
            }) {
                anyhow::bail!(
                    "unconditional refetch for {} received another 304/reuse response: {}",
                    manifest_item.canonical_uri,
                    warning.message
                );
            }
            anyhow::bail!(
                "unconditional refetch for {} returned no document",
                manifest_item.canonical_uri
            );
        }
    };
    let reuse_required = reacquired
        .metadata
        .get("web_reuse_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if reuse_required
        || matches!(
            &reacquired.content_ref,
            ContentRef::External { uri, .. } if uri.starts_with("reuse://")
        )
    {
        anyhow::bail!(
            "unconditional refetch for {} returned 304/reuse instead of content",
            manifest_item.canonical_uri
        );
    }
    Ok(reacquired)
}
