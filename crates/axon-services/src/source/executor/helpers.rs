//! Small schema and provider helpers for the canonical source pipeline.

use axon_adapters::SourceEnricher;
use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use std::sync::Arc;

use super::SourcePipelineInput;
use crate::context::TargetLocalSourceRuntime;
use crate::source::events::SourceEventEmitter;
use crate::source::result_map::IndexCounts;

use super::vectorize::VectorizeResult;

pub(super) async fn unchanged_result(
    ledger: &dyn LedgerStore,
    input: &SourcePipelineInput<'_>,
    manifest: SourceManifest,
    diff: &SourceManifestDiff,
    previous: Option<&SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    let generation = diff
        .previous_generation
        .clone()
        .ok_or_else(|| anyhow::anyhow!("unchanged source has no committed generation"))?;
    let counts = previous
        .map(preserved_source_counts)
        .unwrap_or(SourceCounts {
            items_total: manifest.items.len() as u64,
            items_changed: 0,
            documents_total: manifest.items.len() as u64,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: manifest
                .items
                .iter()
                .map(|item| item.size_bytes.unwrap_or(0))
                .sum(),
        });
    ledger
        .upsert_source(super::metadata::source_summary(
            input,
            LifecycleStatus::Completed,
            counts,
            previous,
        ))
        .await?;
    Ok(IndexCounts {
        job_id: input.plan.job_id,
        source_id: manifest.source_id.clone(),
        generation,
        items_discovered: manifest.items.len() as u64,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        published_manifest: Some(manifest),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    })
}

pub(super) fn collection_spec(collection: &str, dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: [
            "source_id",
            "source_generation",
            "source_item_key",
            "document_id",
            "chunk_id",
        ]
        .into_iter()
        .map(payload_index)
        .collect(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

pub(super) fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

pub(super) fn apply_max_items(manifest: &mut SourceManifest, max_items: Option<u64>) {
    if let Some(limit) = max_items.and_then(|value| usize::try_from(value).ok()) {
        manifest.items.truncate(limit);
    }
}

/// Split a diff's added+modified items into batches of at most `batch_size`,
/// preserving every other diff field (header/source_id/generations) on each
/// batch so `adapter.acquire(&plan, &batch)` sees a well-formed
/// `SourceManifestDiff`. Mirrors the pre-collapse `local_source_vectorize.rs`
/// / `web_source/vectorize_helpers.rs` `changed_diff_batches` helpers those
/// two pipelines already used — now the one copy every source family
/// (including local) streams acquisition through (finding C1: `executor` used
/// to be the only pipeline that acquired an entire changed generation in one
/// unbounded call).
pub(super) fn batch_changed_diff(
    diff: &SourceManifestDiff,
    batch_size: usize,
) -> impl Iterator<Item = SourceManifestDiff> + '_ {
    let batch_size = batch_size.max(1);
    let mut added = diff.added.iter();
    let mut modified = diff.modified.iter();
    let mut added_exhausted = false;

    std::iter::from_fn(move || {
        let mut batch = empty_diff_like(diff);
        while batch.added.len() + batch.modified.len() < batch_size {
            if !added_exhausted {
                if let Some(item) = added.next() {
                    batch.added.push(item.clone());
                    continue;
                }
                added_exhausted = true;
            }
            let Some(item) = modified.next() else {
                break;
            };
            batch.modified.push(item.clone());
        }

        if batch.added.is_empty() && batch.modified.is_empty() {
            return None;
        }
        batch.counts.added = batch.added.len() as u64;
        batch.counts.modified = batch.modified.len() as u64;
        Some(batch)
    })
}

fn empty_diff_like(diff: &SourceManifestDiff) -> SourceManifestDiff {
    SourceManifestDiff {
        header: diff.header.clone(),
        source_id: diff.source_id.clone(),
        previous_generation: diff.previous_generation.clone(),
        next_generation: diff.next_generation.clone(),
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

pub(super) fn manifest_has_changes(diff: &SourceManifestDiff) -> bool {
    !diff.added.is_empty()
        || !diff.modified.is_empty()
        || !diff.removed.is_empty()
        || !diff.failed.is_empty()
}

#[cfg(test)]
pub(super) fn publication_config_matches(
    manifest: &SourceManifest,
    config_snapshot_id: &ConfigSnapshotId,
) -> bool {
    publication_config_metadata_matches(&manifest.metadata, config_snapshot_id)
}

pub(super) fn publication_config_metadata_matches(
    metadata: &MetadataMap,
    config_snapshot_id: &ConfigSnapshotId,
) -> bool {
    metadata
        .get(super::PUBLICATION_CONFIG_KEY)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|stored| stored == config_snapshot_id.0.as_str())
}

pub(super) fn force_publication_refresh(diff: &mut SourceManifestDiff) {
    let unchanged = std::mem::take(&mut diff.unchanged);
    diff.counts.modified = diff.counts.modified.saturating_add(unchanged.len() as u64);
    diff.counts.unchanged = 0;
    diff.modified.extend(unchanged);
}

pub(super) fn terminal_source_counts(
    previous: Option<&SourceSummary>,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
    vectorized: &VectorizeResult,
) -> SourceCounts {
    let changed = diff.counts.added + diff.counts.modified + diff.counts.removed;
    let bytes_total = manifest
        .items
        .iter()
        .map(|item| item.size_bytes.unwrap_or(0))
        .sum();
    if diff.counts.unchanged == 0 {
        return SourceCounts {
            items_total: manifest.items.len() as u64,
            items_changed: changed,
            documents_total: vectorized.documents_prepared,
            chunks_total: vectorized.chunks_prepared,
            vector_points_total: vectorized.points_written,
            bytes_total,
        };
    }

    let prior = previous.map(|source| &source.counts);
    let previous_items = prior.map_or(0, |counts| counts.items_total);
    let retained = |value: u64| {
        if previous_items == 0 {
            return 0;
        }
        let numerator = u128::from(value)
            .saturating_mul(u128::from(diff.counts.unchanged))
            .saturating_add(u128::from(previous_items / 2));
        u64::try_from(numerator / u128::from(previous_items)).unwrap_or(u64::MAX)
    };
    SourceCounts {
        items_total: manifest.items.len() as u64,
        items_changed: changed,
        documents_total: retained(prior.map_or(0, |counts| counts.documents_total))
            .saturating_add(vectorized.documents_prepared),
        chunks_total: retained(prior.map_or(0, |counts| counts.chunks_total))
            .saturating_add(vectorized.chunks_prepared),
        vector_points_total: retained(prior.map_or(0, |counts| counts.vector_points_total))
            .saturating_add(vectorized.points_written),
        bytes_total,
    }
}

pub(super) fn empty_source_counts() -> SourceCounts {
    SourceCounts {
        items_total: 0,
        items_changed: 0,
        documents_total: 0,
        chunks_total: 0,
        vector_points_total: 0,
        bytes_total: 0,
    }
}

pub(super) fn preserved_source_counts(source: &SourceSummary) -> SourceCounts {
    SourceCounts {
        items_changed: 0,
        ..source.counts.clone()
    }
}

pub(super) async fn ensure_providers_ready(
    runtime: &TargetLocalSourceRuntime,
) -> anyhow::Result<()> {
    crate::reserved_call::ensure_source_providers_ready(runtime).await?;
    Ok(())
}

pub(super) fn timestamp() -> Timestamp {
    Timestamp::from(chrono::Utc::now())
}

pub(super) fn stage_counts(output: &IndexCounts) -> StageCounts {
    StageCounts {
        items_total: Some(output.items_discovered),
        items_done: output.items_discovered,
        documents_total: Some(output.documents_prepared),
        documents_done: output.documents_prepared,
        chunks_total: Some(output.chunks_prepared),
        chunks_done: output.chunks_prepared,
        bytes_total: None,
        bytes_done: 0,
    }
}

pub(super) async fn record_running_phase(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    phase: PipelinePhase,
    message: &str,
) -> anyhow::Result<()> {
    runtime
        .jobs
        .update_status(JobStatusUpdate {
            job_id: input.plan.job_id,
            source_id: Some(input.plan.route.source.source_id.clone()),
            status: LifecycleStatus::Running,
            phase,
            stage_id: None,
            counts: None,
            current: Some(ProgressCurrent {
                source_item_key: None,
                document_id: None,
                chunk_id: None,
                adapter: Some(input.plan.route.adapter.name.clone()),
                provider: None,
                message: Some(message.to_string()),
            }),
            message: Some(message.to_string()),
            error: None,
        })
        .await?;
    emitter.running(phase, message).await;
    Ok(())
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;

pub(super) async fn enrich(
    enricher: Arc<dyn SourceEnricher>,
    plan: &SourcePlan,
    items: &[AcquiredSourceItem],
) -> anyhow::Result<std::collections::BTreeMap<SourceItemKey, SourceEnrichment>> {
    let mut output = std::collections::BTreeMap::new();
    for item in items {
        let result = enricher.enrich(plan, item).await?;
        output.insert(item.manifest_item.source_item_key.clone(), result);
    }
    Ok(output)
}

pub(super) fn apply_enrichments(
    documents: &mut [SourceDocument],
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) {
    for document in documents {
        if let Some(enrichment) = enrichments.get(&document.source_item_key) {
            document.parser_hints.extend(enrichment.parse_hints.clone());
            document.chunk_hints.extend(enrichment.chunk_hints.clone());
            document.metadata.0.extend(enrichment.metadata.0.clone());
        }
    }
}

pub(super) fn take_enrichment_graph_candidates(
    enrichments: &mut std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) -> std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>> {
    enrichments
        .iter_mut()
        .filter_map(|(key, enrichment)| {
            if enrichment.graph_candidates.is_empty() {
                None
            } else {
                Some((
                    key.clone(),
                    std::mem::take(&mut enrichment.graph_candidates),
                ))
            }
        })
        .collect()
}

/// Build the terminal `SourceError` for a failed canonical source pipeline run.
///
/// This is persisted straight into `jobs.last_error_json` — a column with no
/// automatic redaction pass (unlike `job_events`/`details_json`, which run
/// through `redact_metadata`) — so secrets must be scrubbed here before
/// either field is populated. `message` keeps anyhow's top-context frame
/// only (unchanged shape from before); `cause` carries the full `.context()`
/// chain via `{error:#}`, and only when it actually adds something beyond
/// `message`, so a single-frame error doesn't get a pointless duplicate.
pub(super) fn terminal_source_error(error: &anyhow::Error) -> SourceError {
    let message = axon_core::redact::redact_operational_secrets(&error.to_string());
    let full_chain = axon_core::redact::redact_operational_secrets(&format!("{error:#}"));
    let cause = (full_chain != message).then_some(full_chain);
    SourceError {
        code: "source.index_failed".to_string(),
        severity: Severity::Failed,
        message,
        source_item_key: None,
        retryable: false,
        provider_id: None,
        cause,
    }
}
