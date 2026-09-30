//! Capability-driven conditional-request reuse for the shared source runner.

use std::collections::{BTreeMap, BTreeSet};

use axon_adapters::ReusePolicy;
use axon_api::source::*;

use super::SourcePipelineInput;
use crate::context::TargetLocalSourceRuntime;

mod eligibility;

const PRIOR_ETAG: &str = "web_prior_etag";
const ETAG: &str = "web_etag";
const PRIOR_LAST_MODIFIED: &str = "web_prior_last_modified";
const LAST_MODIFIED: &str = "web_last_modified";
const REUSE_REQUIRED: &str = "web_reuse_required";

pub(super) struct ResolvedAcquisition {
    pub(super) acquisition: SourceAcquisition,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) cached_documents_to_prepare: Vec<SourceDocument>,
    pub(super) cached_items_to_enrich: Vec<AcquiredSourceItem>,
}

pub(super) async fn overlay_trusted_validators(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    mut diff: SourceManifestDiff,
) -> anyhow::Result<SourceManifestDiff> {
    if input.plan.request.refresh == SourceRefreshPolicy::Force {
        return Ok(diff);
    }
    if input.adapter.reuse_policy() != ReusePolicy::ConditionalRequest {
        return Ok(diff);
    }
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(diff);
    };
    let modified_keys = diff
        .modified
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect::<Vec<_>>();
    if modified_keys.is_empty() {
        return Ok(diff);
    }
    let previous_items = runtime
        .ledger
        .get_manifest_items(diff.source_id.clone(), previous_generation, modified_keys)
        .await?
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect::<BTreeMap<_, _>>();
    for item in &mut diff.modified {
        let Some(previous) = previous_items.get(&item.source_item_key) else {
            continue;
        };
        copy_validator(previous, item, ETAG, PRIOR_ETAG);
        copy_validator(previous, item, LAST_MODIFIED, PRIOR_LAST_MODIFIED);
    }
    Ok(diff)
}

pub(super) async fn resolve_acquisition(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    mut acquisition: SourceAcquisition,
) -> anyhow::Result<ResolvedAcquisition> {
    // A forced refresh is also the canonical cold-run path: refetch,
    // renormalize, rechunk, and re-embed even when the bytes match the prior
    // generation. Reusing cached documents here made `refresh=force` only a
    // network refresh and produced misleading end-to-end benchmarks.
    if input.plan.request.refresh == SourceRefreshPolicy::Force {
        return Ok(ResolvedAcquisition {
            acquisition,
            reused_item_keys: Vec::new(),
            cached_documents_to_prepare: Vec::new(),
            cached_items_to_enrich: Vec::new(),
        });
    }
    if input.adapter.reuse_policy() != ReusePolicy::ConditionalRequest {
        return Ok(ResolvedAcquisition {
            acquisition,
            reused_item_keys: Vec::new(),
            cached_documents_to_prepare: Vec::new(),
            cached_items_to_enrich: Vec::new(),
        });
    }

    let previous_items = previous_manifest_items(runtime, diff).await?;
    let reusable = eligibility::prepared_reuse_keys(runtime, input, diff).await?;
    let mut refreshed_items = BTreeMap::new();
    let mut fetched = Vec::new();
    let mut reused_item_keys = Vec::new();
    let mut cached_documents_to_prepare = Vec::new();
    let mut cached_items_to_enrich = Vec::new();
    for item in std::mem::take(&mut acquisition.fetched_items) {
        refreshed_items.insert(
            item.manifest_item.source_item_key.clone(),
            item.manifest_item.clone(),
        );
        if !reuse_required(&item) {
            let item_key = item.manifest_item.source_item_key.clone();
            let same_content = previous_items.get(&item_key).is_some_and(|previous| {
                previous.content_hash.is_some()
                    && previous.content_hash == item.manifest_item.content_hash
            });
            if same_content
                && reusable.contains(&item_key)
                && reuse_cached_document(runtime, diff, &item_key)
                    .await?
                    .is_some()
            {
                reused_item_keys.push(item_key);
                continue;
            }
            // A byte-identical 200 response can only skip normalization when the
            // previous normalized document was successfully advanced into the
            // new generation's cache. Otherwise process the fetched body so this
            // generation becomes a valid cache source for the next 304.
            fetched.push(item);
            continue;
        }
        let item_key = item.manifest_item.source_item_key.clone();
        if let Some(document) = reuse_cached_document(runtime, diff, &item_key).await? {
            if reusable.contains(&item_key) {
                reused_item_keys.push(item_key);
            } else {
                let mut cached_item = item;
                cached_item.content_ref = document.content.clone();
                cached_items_to_enrich.push(cached_item);
                cached_documents_to_prepare.push(document);
            }
        } else {
            acquisition.header.warnings.push(SourceWarning {
                code: "source.reuse.cache_miss_refetch".to_string(),
                severity: Severity::Warning,
                message: "conditional response has no usable cached body; refetching".to_owned(),
                source_item_key: Some(item_key),
                retryable: true,
            });
            let canonical_uri = item.manifest_item.canonical_uri.clone();
            let reacquired = refetch_unconditionally(input, diff, item.manifest_item).await?;
            let item = merge_reacquired(&mut acquisition, reacquired, &canonical_uri)?;
            refreshed_items.insert(
                item.manifest_item.source_item_key.clone(),
                item.manifest_item.clone(),
            );
            fetched.push(item);
        }
    }

    for manifest_item in &mut acquisition.manifest.items {
        if let Some(refreshed) = refreshed_items.remove(&manifest_item.source_item_key) {
            *manifest_item = refreshed;
        }
    }
    acquisition.fetched_items = fetched;
    Ok(ResolvedAcquisition {
        acquisition,
        reused_item_keys,
        cached_documents_to_prepare,
        cached_items_to_enrich,
    })
}

async fn previous_manifest_items(
    runtime: &TargetLocalSourceRuntime,
    diff: &SourceManifestDiff,
) -> anyhow::Result<BTreeMap<SourceItemKey, ManifestItem>> {
    let Some(generation) = diff.previous_generation.clone() else {
        return Ok(BTreeMap::new());
    };
    let keys = diff
        .added
        .iter()
        .chain(&diff.modified)
        .map(|item| item.source_item_key.clone())
        .collect();
    Ok(runtime
        .ledger
        .get_manifest_items(diff.source_id.clone(), generation, keys)
        .await?
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect())
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
    diff: SourceManifestDiff,
    reused_item_keys: &BTreeSet<SourceItemKey>,
) -> SourceManifestDiff {
    if reused_item_keys.is_empty() {
        return diff;
    }
    let mut adjusted = diff;
    let mut modified = Vec::with_capacity(adjusted.modified.len());
    for item in adjusted.modified.drain(..) {
        if reused_item_keys.contains(&item.source_item_key) {
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
) -> anyhow::Result<Option<SourceDocument>> {
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(None);
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
        return Ok(None);
    };
    if cached.document.source_id != diff.source_id
        || cached.document.source_item_key != *item_key
        || !matches!(
            cached.document.content,
            ContentRef::InlineText { .. } | ContentRef::InlineBytes { .. }
        )
        || cached
            .document
            .metadata
            .get(CONTENT_OMISSION_METADATA_KEY)
            .is_some()
    {
        return Ok(None);
    }
    cached.document.metadata.remove("source_generation");
    cached.document.metadata.remove("committed_generation");
    cached.cached_at = timestamp();
    let document = cached.document.clone();
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
    Ok(Some(document))
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
