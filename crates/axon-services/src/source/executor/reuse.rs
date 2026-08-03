//! Capability-driven conditional-request reuse for the shared source runner.

use std::collections::{BTreeMap, BTreeSet};

use axon_adapters::ReusePolicy;
use axon_api::source::*;

use super::SourcePipelineInput;
use crate::context::TargetLocalSourceRuntime;

const PRIOR_ETAG: &str = "web_prior_etag";
const ETAG: &str = "web_etag";
const PRIOR_LAST_MODIFIED: &str = "web_prior_last_modified";
const LAST_MODIFIED: &str = "web_last_modified";
const REUSE_REQUIRED: &str = "web_reuse_required";

pub(super) struct ResolvedAcquisition {
    pub(super) acquisition: SourceAcquisition,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
}

pub(super) async fn overlay_trusted_validators(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
) -> anyhow::Result<SourceManifestDiff> {
    if input.adapter.reuse_policy() != ReusePolicy::ConditionalRequest {
        return Ok(diff.clone());
    }
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(diff.clone());
    };
    let Some(previous_manifest) = runtime
        .ledger
        .get_manifest(diff.source_id.clone(), previous_generation)
        .await?
    else {
        return Ok(diff.clone());
    };
    let previous_items = previous_manifest
        .items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let mut adjusted = diff.clone();
    for item in &mut adjusted.modified {
        let Some(previous) = previous_items.get(&item.source_item_key) else {
            continue;
        };
        copy_validator(previous, item, ETAG, PRIOR_ETAG);
        copy_validator(previous, item, LAST_MODIFIED, PRIOR_LAST_MODIFIED);
    }
    Ok(adjusted)
}

pub(super) async fn resolve_acquisition(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    mut acquisition: SourceAcquisition,
) -> anyhow::Result<ResolvedAcquisition> {
    if input.adapter.reuse_policy() != ReusePolicy::ConditionalRequest {
        return Ok(ResolvedAcquisition {
            acquisition,
            reused_item_keys: Vec::new(),
        });
    }

    let mut fetched = Vec::new();
    let mut reused_item_keys = Vec::new();
    for item in std::mem::take(&mut acquisition.fetched_items) {
        if !reuse_required(&item) {
            fetched.push(item);
            continue;
        }
        let item_key = item.manifest_item.source_item_key.clone();
        if reuse_cached_document(runtime, diff, &item_key).await? {
            reused_item_keys.push(item_key);
        } else {
            acquisition.header.warnings.push(SourceWarning {
                code: "source.reuse.cache_miss_refetch".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "conditional reuse for {} had no cached committed document; refetching",
                    item.manifest_item.canonical_uri
                ),
                source_item_key: Some(item_key),
                retryable: true,
            });
            let canonical_uri = item.manifest_item.canonical_uri.clone();
            let reacquired = refetch_unconditionally(input, diff, item.manifest_item).await?;
            fetched.push(merge_reacquired(
                &mut acquisition,
                reacquired,
                &canonical_uri,
            )?);
        }
    }

    acquisition.fetched_items = fetched;
    Ok(ResolvedAcquisition {
        acquisition,
        reused_item_keys,
    })
}

pub(super) async fn normalize_acquisition(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    acquisition: SourceAcquisition,
) -> anyhow::Result<StageExecutionResult<Vec<SourceDocument>>> {
    let conditional = input.adapter.reuse_policy() == ReusePolicy::ConditionalRequest;
    let inherited_warnings = acquisition.header.warnings.clone();
    let mut normalized = input.adapter.normalize(&input.plan, acquisition).await?;
    normalized.header.warnings.splice(0..0, inherited_warnings);
    if conditional {
        cache_documents(runtime, diff, &normalized.data).await?;
    }
    Ok(normalized)
}

pub(super) fn apply_reused_items(
    diff: &SourceManifestDiff,
    reused_item_keys: &[SourceItemKey],
) -> SourceManifestDiff {
    if reused_item_keys.is_empty() {
        return diff.clone();
    }
    let reused = reused_item_keys.iter().cloned().collect::<BTreeSet<_>>();
    let mut adjusted = diff.clone();
    let mut modified = Vec::with_capacity(adjusted.modified.len());
    for item in adjusted.modified.drain(..) {
        if reused.contains(&item.source_item_key) {
            adjusted.unchanged.push(item);
        } else {
            modified.push(item);
        }
    }
    adjusted.modified = modified;
    adjusted.counts.modified = adjusted.modified.len() as u64;
    adjusted.counts.unchanged = adjusted.unchanged.len() as u64;
    adjusted
}

fn copy_validator(previous: &ManifestItem, current: &mut ManifestItem, source: &str, target: &str) {
    if let Some(value) = previous.metadata.get(source) {
        current.metadata.insert(target.to_string(), value.clone());
    }
}

fn reuse_required(item: &AcquiredSourceItem) -> bool {
    item.metadata
        .get(REUSE_REQUIRED)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || matches!(
            &item.content_ref,
            ContentRef::External { uri, .. } if uri.starts_with("reuse://")
        )
}

fn merge_reacquired(
    acquisition: &mut SourceAcquisition,
    mut reacquired: SourceAcquisition,
    canonical_uri: &str,
) -> anyhow::Result<AcquiredSourceItem> {
    if reacquired.fetched_items.len() != 1 {
        anyhow::bail!(
            "unconditional refetch for {canonical_uri} returned {} items; expected exactly one",
            reacquired.fetched_items.len()
        );
    }
    let acquired = reacquired
        .fetched_items
        .pop()
        .expect("refetch item count checked above");
    if reuse_required(&acquired) {
        anyhow::bail!("unconditional refetch for {canonical_uri} returned another reuse response");
    }
    acquisition
        .header
        .warnings
        .append(&mut reacquired.header.warnings);
    acquisition.artifacts.append(&mut reacquired.artifacts);
    Ok(acquired)
}

async fn reuse_cached_document(
    runtime: &TargetLocalSourceRuntime,
    diff: &SourceManifestDiff,
    item_key: &SourceItemKey,
) -> anyhow::Result<bool> {
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(false);
    };
    let Some(mut cached) = runtime
        .document_cache
        .get(DocumentCacheKey {
            source_id: diff.source_id.clone(),
            source_item_key: item_key.clone(),
            generation: Some(previous_generation),
        })
        .await?
    else {
        return Ok(false);
    };
    cached.document.metadata.remove("source_generation");
    cached.document.metadata.remove("committed_generation");
    cached.cached_at = timestamp();
    runtime
        .document_cache
        .put(
            DocumentCacheKey {
                source_id: diff.source_id.clone(),
                source_item_key: item_key.clone(),
                generation: Some(diff.next_generation.clone()),
            },
            cached,
        )
        .await?;
    Ok(true)
}

async fn cache_documents(
    runtime: &TargetLocalSourceRuntime,
    diff: &SourceManifestDiff,
    documents: &[SourceDocument],
) -> anyhow::Result<()> {
    for document in documents {
        runtime
            .document_cache
            .put(
                DocumentCacheKey {
                    source_id: document.source_id.clone(),
                    source_item_key: document.source_item_key.clone(),
                    generation: Some(diff.next_generation.clone()),
                },
                CachedDocument {
                    document: document.clone(),
                    cached_at: timestamp(),
                },
            )
            .await?;
    }
    Ok(())
}

async fn refetch_unconditionally(
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    mut item: ManifestItem,
) -> anyhow::Result<SourceAcquisition> {
    item.metadata.remove(PRIOR_ETAG);
    item.metadata.remove(PRIOR_LAST_MODIFIED);
    let mut plan = input.plan.clone();
    plan.route
        .validated_options
        .values
        .insert("etag_conditional".to_string(), serde_json::json!(false));
    plan.route
        .validated_options
        .values
        .insert("cache_policy".to_string(), serde_json::json!("bypass"));
    Ok(input
        .adapter
        .acquire(&plan, &single_item_diff(diff, item))
        .await?)
}

fn single_item_diff(diff: &SourceManifestDiff, item: ManifestItem) -> SourceManifestDiff {
    SourceManifestDiff {
        header: diff.header.clone(),
        source_id: diff.source_id.clone(),
        previous_generation: diff.previous_generation.clone(),
        next_generation: diff.next_generation.clone(),
        added: Vec::new(),
        modified: vec![item],
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
    }
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
#[path = "reuse_tests.rs"]
mod tests;
