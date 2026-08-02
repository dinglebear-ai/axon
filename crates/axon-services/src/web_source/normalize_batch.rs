use axon_adapters::{SourceAdapter, web};
use axon_api::source::*;

use super::WebSourceIndexInput;
use super::artifacts::{WebArtifactIndex, store_clean_outputs, store_warc_artifact};
use super::normalize::{NormalizedWebDocuments, refetch_without_conditional};
use super::reuse;
use super::run::WebAdapterRun;

pub(super) struct NormalizationBatch {
    warnings: Vec<SourceWarning>,
    artifact_index: WebArtifactIndex,
    artifacts: Vec<ArtifactRef>,
    documents_to_cache: Vec<SourceDocument>,
    fetched_items: Vec<AcquiredSourceItem>,
    reused_item_keys: Vec<SourceItemKey>,
}

impl NormalizationBatch {
    pub(super) async fn collect(
        input: &WebSourceIndexInput,
        run: &WebAdapterRun,
        diff: &SourceManifestDiff,
        acquisition: &mut SourceAcquisition,
    ) -> anyhow::Result<Self> {
        let mut batch = Self {
            warnings: acquisition.header.warnings.clone(),
            artifact_index: WebArtifactIndex::default(),
            artifacts: Vec::new(),
            documents_to_cache: Vec::new(),
            fetched_items: Vec::new(),
            reused_item_keys: Vec::new(),
        };
        for artifact in store_warc_artifact(input, run, &acquisition.fetched_items).await? {
            batch.artifact_index.push_generation(artifact.clone());
            batch.artifacts.push(artifact);
        }
        for item in std::mem::take(&mut acquisition.fetched_items) {
            if !reuse_required(&item) {
                batch.fetched_items.push(item);
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
                batch
                    .reused_item_keys
                    .push(item.manifest_item.source_item_key);
                batch.documents_to_cache.push(reused.document);
                continue;
            }
            batch.warnings.push(cache_miss_warning(&item.manifest_item));
            batch
                .fetched_items
                .push(refetch_without_conditional(input, run, diff, item.manifest_item).await?);
        }
        Ok(batch)
    }

    pub(super) async fn finish(
        mut self,
        input: &WebSourceIndexInput,
        run: &WebAdapterRun,
        diff: &SourceManifestDiff,
        mut acquisition: SourceAcquisition,
    ) -> anyhow::Result<NormalizedWebDocuments> {
        let mut documents = Vec::new();
        let mut inline = None;
        if !self.fetched_items.is_empty() {
            acquisition.fetched_items = self.fetched_items;
            let adapter = web::WebSourceAdapter::new(
                std::sync::Arc::clone(&input.fetch_provider),
                std::sync::Arc::clone(&input.render_provider),
            );
            let normalized = adapter.normalize(&run.plan, acquisition).await?.data;
            let clean_output = store_clean_outputs(input, &normalized).await?;
            self.artifacts.extend(clean_output.artifacts);
            self.artifact_index.merge(clean_output.artifact_index);
            inline = clean_output.inline;
            self.documents_to_cache.extend(normalized.clone());
            documents = normalized;
        }
        reuse::cache_documents(
            input.document_cache.as_ref(),
            &run.source_id,
            &diff.next_generation,
            &self.documents_to_cache,
        )
        .await?;
        Ok(NormalizedWebDocuments {
            documents,
            warnings: self.warnings,
            reused_item_keys: self.reused_item_keys,
            artifacts: self.artifacts,
            inline,
            artifact_index: self.artifact_index,
        })
    }
}

fn reuse_required(item: &AcquiredSourceItem) -> bool {
    item.metadata
        .get("web_reuse_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn cache_miss_warning(item: &ManifestItem) -> SourceWarning {
    SourceWarning {
        code: "web.reuse.cache_miss_refetch".to_string(),
        severity: Severity::Warning,
        message: format!(
            "conditional 304 for {} had no cached committed document; refetching before publish",
            item.canonical_uri
        ),
        source_item_key: Some(item.source_item_key.clone()),
        retryable: true,
    }
}
