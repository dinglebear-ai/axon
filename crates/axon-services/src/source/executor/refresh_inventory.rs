//! Rebuild known HTTP inventory without treating discovery gaps as deletions.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const REBUILD_KEY: &str = "axon_refresh_rebuild_inventory";

pub(super) async fn prepare(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    manifest: &mut SourceManifest,
    diff: &mut SourceManifestDiff,
    unvisited: &BTreeSet<SourceItemKey>,
) -> anyhow::Result<bool> {
    let compatible = match diff.previous_generation.as_ref() {
        Some(generation) => runtime
            .ledger
            .get_manifest_metadata(manifest.source_id.clone(), generation.clone())
            .await?
            .is_some_and(|metadata| {
                publication_config_metadata_matches(
                    &metadata,
                    &retention::processing_identity(runtime, input),
                )
            }),
        None => false,
    };
    let rebuilding = rebuild_retained(input, manifest, diff, unvisited, compatible)?;
    anyhow::ensure!(
        compatible || diff.previous_generation.is_none() || unvisited.is_empty() || rebuilding,
        "partial refresh cannot change processing configuration; complete refresh required"
    );
    if !rebuilding {
        retention::validate_retained(runtime.ledger.as_ref(), diff, unvisited).await?;
    }
    Ok(compatible)
}

pub(super) fn rebuild_retained(
    input: &SourcePipelineInput<'_>,
    manifest: &mut SourceManifest,
    diff: &mut SourceManifestDiff,
    unvisited: &BTreeSet<SourceItemKey>,
    compatible: bool,
) -> anyhow::Result<bool> {
    manifest.metadata.remove(REBUILD_KEY);
    if unvisited.is_empty()
        || input.adapter.reuse_policy() != axon_adapters::ReusePolicy::ConditionalRequest
        || (compatible && input.plan.request.refresh != SourceRefreshPolicy::Force)
    {
        return Ok(false);
    }
    let required = manifest.items.len() as u64;
    if let Some(limit) = input.plan.limits.effective.max_items
        && required > limit
    {
        return Err(ApiError::new(
            "source.refresh.inventory_limit",
            ErrorStage::Diffing,
            format!(
                "refresh must reprocess {required} known pages, exceeding the {limit}-item limit; raise max_items/max_pages to at least {required} or unset the limit and retry; the committed generation is unchanged"
            ),
        )
        .with_source_id(manifest.source_id.0.clone())
        .with_context("required_items", required.to_string())
        .with_context("max_items", limit.to_string())
        .into());
    }
    // Reacquire every known item under one policy. Never carry old prepared
    // output into a generation with different processing settings.
    force_publication_refresh(diff);
    for item in &mut diff.modified {
        // These point at the old generation's ledger-owned output. Acquisition
        // metadata must not copy them into newly normalized vector payloads.
        item.metadata
            .remove(crate::source::output::CACHE_KEY_METADATA_KEY);
        item.metadata
            .remove(crate::source::output::ARTIFACT_METADATA_KEY);
    }
    manifest.metadata.insert(REBUILD_KEY.into(), true.into());
    tracing::info!(source_id = %manifest.source_id.0, known_items = required,
        retained_items = unvisited.len(), config_compatible = compatible,
        refresh = ?input.plan.request.refresh, "reprocessing retained HTTP source inventory");
    Ok(true)
}

pub(super) async fn validate_rebuilt(
    ledger: &dyn LedgerStore,
    manifest: &SourceManifest,
) -> anyhow::Result<()> {
    if manifest
        .metadata
        .get(REBUILD_KEY)
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Ok(());
    }
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let statuses = ledger
        .document_statuses_for_items(manifest.source_id.clone(), keys)
        .await?;
    let mut eligible_by_item = BTreeMap::new();
    for status in statuses {
        let eligible = retention::eligible(&status, &manifest.generation);
        eligible_by_item
            .entry(status.source_item_key)
            .and_modify(|all_eligible| *all_eligible &= eligible)
            .or_insert(eligible);
    }
    for item in &manifest.items {
        if eligible_by_item.get(&item.source_item_key) != Some(&true) {
            return Err(ApiError::new(
                "source.refresh.inventory_incomplete",
                ErrorStage::Publishing,
                "retained page could not be reprocessed; inspect acquisition errors and retry after recovery; the committed generation is unchanged",
            ).with_source_id(manifest.source_id.0.clone())
                .with_source_item_key(item.source_item_key.0.clone()).into());
        }
    }
    Ok(())
}
