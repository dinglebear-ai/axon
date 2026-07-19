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
use super::artifacts::{WebArtifactIndex, store_clean_outputs, store_warc_artifact};
use super::reuse;
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
) -> anyhow::Result<NormalizedWebDocuments> {
    let adapter = web::WebSourceAdapter::new(
        std::sync::Arc::clone(&input.fetch_provider),
        std::sync::Arc::clone(&input.render_provider),
    );
    let mut acquisition = adapter.acquire(&run.plan, diff).await?;
    let mut warnings = acquisition.header.warnings.clone();
    let mut artifact_index = WebArtifactIndex::default();
    let mut artifacts = Vec::new();
    for artifact in store_warc_artifact(input, run, &acquisition.fetched_items).await? {
        artifact_index.push_generation(artifact.clone());
        artifacts.push(artifact);
    }
    let mut documents = Vec::new();
    let mut documents_to_cache = Vec::new();
    let mut fetched_items = Vec::new();
    let mut reused_item_keys = Vec::new();

    for item in std::mem::take(&mut acquisition.fetched_items) {
        let reuse_required = item
            .metadata
            .get("web_reuse_required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !reuse_required {
            fetched_items.push(item);
            continue;
        }

        if let Some(reused) = reuse::load_reused_web_document(
            input.document_cache.as_ref(),
            &run.source_id,
            diff.previous_generation.as_ref(),
            &item.manifest_item.source_item_key,
            &diff.next_generation,
        )
        .await?
        {
            reused_item_keys.push(item.manifest_item.source_item_key.clone());
            documents_to_cache.push(reused.document);
            continue;
        }

        warnings.push(SourceWarning {
            code: "web.reuse.cache_miss_refetch".to_string(),
            severity: Severity::Warning,
            message: format!(
                "conditional 304 for {} had no cached committed document; refetching before publish",
                item.manifest_item.canonical_uri
            ),
            source_item_key: Some(item.manifest_item.source_item_key.clone()),
            retryable: true,
        });
        fetched_items
            .push(refetch_without_conditional(input, run, diff, item.manifest_item).await?);
    }

    if !fetched_items.is_empty() {
        acquisition.fetched_items = fetched_items;
        let normalized = adapter.normalize(&run.plan, acquisition).await?.data;
        let clean_output = store_clean_outputs(input, &normalized).await?;
        artifacts.extend(clean_output.artifacts);
        artifact_index.merge(clean_output.artifact_index);
        let inline = clean_output.inline;
        documents_to_cache.extend(normalized.clone());
        documents.extend(normalized);
        reuse::cache_documents(
            input.document_cache.as_ref(),
            &run.source_id,
            &diff.next_generation,
            &documents_to_cache,
        )
        .await?;
        return Ok(NormalizedWebDocuments {
            documents,
            warnings,
            reused_item_keys,
            artifacts,
            inline,
            artifact_index,
        });
    }

    reuse::cache_documents(
        input.document_cache.as_ref(),
        &run.source_id,
        &diff.next_generation,
        &documents_to_cache,
    )
    .await?;
    Ok(NormalizedWebDocuments {
        documents,
        warnings,
        reused_item_keys,
        artifacts,
        inline: None,
        artifact_index,
    })
}

async fn refetch_without_conditional(
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
