//! Small schema and provider helpers for the non-web pipeline.

use axon_adapters::SourceEnricher;
use axon_api::{ApiError, source::*};
use axon_error::ErrorStage;
use axon_ledger::store::LedgerStore;
use std::sync::Arc;

use super::NonWebPipelineInput;
use crate::context::TargetLocalSourceRuntime;
use crate::source::events::SourceEventEmitter;
use crate::source::result_map::IndexCounts;

use super::vectorize::VectorizeResult;

pub(super) async fn unchanged_result(
    ledger: &dyn LedgerStore,
    input: &NonWebPipelineInput<'_>,
    manifest: &SourceManifest,
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
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
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

pub(super) fn manifest_has_changes(diff: &SourceManifestDiff) -> bool {
    !diff.added.is_empty()
        || !diff.modified.is_empty()
        || !diff.removed.is_empty()
        || !diff.failed.is_empty()
}

pub(super) fn publication_config_matches(
    manifest: &SourceManifest,
    config_snapshot_id: &ConfigSnapshotId,
) -> bool {
    manifest
        .metadata
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
    let embedding = runtime.embedding_provider.capabilities().await?;
    let vector = runtime.vector_store.capabilities().await?;
    for capability in [&embedding, &vector] {
        if !matches!(
            capability.health,
            HealthStatus::Healthy | HealthStatus::Degraded
        ) {
            return Err(capability
                .last_error
                .clone()
                .unwrap_or_else(|| {
                    ApiError::new(
                        "provider.not_ready",
                        ErrorStage::Planning,
                        format!("provider {} is not ready", capability.provider_id.0),
                    )
                })
                .into());
        }
    }
    if !vector
        .vector_store
        .as_ref()
        .is_some_and(|capability| capability.generation_publish)
    {
        anyhow::bail!("vector provider does not support source generation publication");
    }
    Ok(())
}

pub(super) fn timestamp() -> Timestamp {
    Timestamp::from(chrono::Utc::now())
}

pub(super) fn stage_counts(output: &IndexCounts) -> StageCounts {
    StageCounts {
        items_total: Some(output.documents_prepared + output.removed),
        items_done: output.documents_prepared,
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
    input: &NonWebPipelineInput<'_>,
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
            current: None,
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

pub(super) fn enrichment_graph_candidates(
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) -> std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>> {
    enrichments
        .iter()
        .filter_map(|(key, enrichment)| {
            (!enrichment.graph_candidates.is_empty())
                .then(|| (key.clone(), enrichment.graph_candidates.clone()))
        })
        .collect()
}
