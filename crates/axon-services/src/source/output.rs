//! Shared source output and durable artifact publication.

use axon_adapters::SourceAdapter;
use axon_api::source::*;
use axon_core::boundary::ArtifactBytesWriteRequest;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call;

pub(crate) const ARTIFACT_METADATA_KEY: &str = "_axon_artifacts";
pub(crate) const CACHE_KEY_METADATA_KEY: &str = "_axon_document_cache_key";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SourceArtifactIndex {
    generation_artifacts: Vec<ArtifactRef>,
    item_artifacts: BTreeMap<SourceItemKey, Vec<ArtifactRef>>,
}

impl SourceArtifactIndex {
    fn is_empty(&self) -> bool {
        self.generation_artifacts.is_empty() && self.item_artifacts.is_empty()
    }

    fn push_generation(&mut self, artifact: ArtifactRef) {
        self.generation_artifacts.push(artifact);
    }

    fn push_item(&mut self, source_item_key: SourceItemKey, artifact: ArtifactRef) {
        self.item_artifacts
            .entry(source_item_key)
            .or_default()
            .push(artifact);
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.generation_artifacts.extend(other.generation_artifacts);
        for (source_item_key, artifacts) in other.item_artifacts {
            self.item_artifacts
                .entry(source_item_key)
                .or_default()
                .extend(artifacts);
        }
    }
}

#[derive(Default)]
pub(crate) struct SourceOutput {
    pub(crate) artifacts: Vec<ArtifactRef>,
    pub(crate) inline: Option<InlineSourceResult>,
    pub(crate) artifact_index: SourceArtifactIndex,
}

impl SourceOutput {
    pub(crate) fn merge(&mut self, mut other: Self) {
        self.artifacts.append(&mut other.artifacts);
        if self.inline.is_none() {
            self.inline = other.inline.take();
        }
        self.artifact_index.merge(other.artifact_index);
    }
}

pub(crate) async fn store_clean_outputs(
    runtime: &TargetLocalSourceRuntime,
    plan: &SourcePlan,
    documents: &[SourceDocument],
) -> anyhow::Result<SourceOutput> {
    let mut output = SourceOutput::default();
    for document in documents {
        let bytes = content_bytes(&document.content)?;
        let size_bytes = bytes.len() as u64;
        if output.inline.is_none() && should_inline(&plan.request.output, size_bytes) {
            output.inline = Some(InlineSourceResult {
                content: Some(document.content.clone()),
                summary: None,
                metadata: document.metadata.clone(),
            });
        }
        if !should_store_artifact(&plan.request.output, size_bytes) {
            continue;
        }
        let mut metadata = document.metadata.clone();
        metadata.insert(
            "producer".to_string(),
            serde_json::json!(plan.route.adapter.name.clone()),
        );
        metadata.insert(
            "source_item_key".to_string(),
            serde_json::json!(document.source_item_key.0.clone()),
        );
        metadata.insert(
            "canonical_uri".to_string(),
            serde_json::json!(document.canonical_uri.clone()),
        );
        let artifact = store_artifact(
            runtime,
            ArtifactKind::NormalizedContent,
            document
                .mime_type
                .clone()
                .unwrap_or_else(|| "text/markdown".to_string()),
            document.content.clone(),
            document.source_id.clone(),
            plan.job_id,
            metadata,
        )
        .await?;
        output
            .artifact_index
            .push_item(document.source_item_key.clone(), artifact.clone());
        output.artifacts.push(artifact);
    }
    Ok(output)
}

pub(crate) async fn store_adapter_archive(
    runtime: &TargetLocalSourceRuntime,
    adapter: &dyn SourceAdapter,
    plan: &SourcePlan,
    items: &[AcquiredSourceItem],
) -> anyhow::Result<SourceOutput> {
    if plan.request.output.artifact_mode == ArtifactMode::None {
        return Ok(SourceOutput::default());
    }
    let Some(mut archive) = adapter.build_archive(plan, items) else {
        return Ok(SourceOutput::default());
    };
    archive
        .metadata
        .insert("producer".to_string(), serde_json::json!(adapter.name()));
    let source_id = plan.route.source.source_id.clone();
    let size_bytes = archive.bytes.len() as u64;
    let handle = reserved_call::put_artifact_bytes(
        runtime,
        ArtifactBytesWriteRequest {
            kind: archive.kind,
            content_type: archive.content_type,
            bytes: archive.bytes,
            source_id: Some(source_id),
            job_id: Some(plan.job_id),
            metadata: archive.metadata,
        },
    )
    .await?;
    let artifact = ArtifactRef {
        artifact_id: handle.artifact_id,
        artifact_kind: handle.artifact_kind,
        uri: handle.uri.unwrap_or_default(),
        size_bytes: Some(size_bytes),
        content_hash: Some(archive.content_hash),
        created_at: timestamp(),
    };
    let mut output = SourceOutput::default();
    output.artifact_index.push_generation(artifact.clone());
    output.artifacts.push(artifact);
    Ok(output)
}

pub(crate) async fn record_artifacts_on_manifest(
    ledger: &dyn axon_ledger::store::LedgerStore,
    manifest: &mut SourceManifest,
    diff: &SourceManifestDiff,
    artifact_index: &SourceArtifactIndex,
) -> anyhow::Result<()> {
    if artifact_index.is_empty() && manifest.items.is_empty() {
        return Ok(());
    }
    let previous_items = previous_manifest_items(ledger, diff).await?;
    put_artifacts(&mut manifest.metadata, &artifact_index.generation_artifacts);
    for item in &mut manifest.items {
        if let Some(artifacts) = artifact_index.item_artifacts.get(&item.source_item_key) {
            put_artifacts(&mut item.metadata, artifacts);
        } else if diff
            .unchanged
            .iter()
            .any(|unchanged| unchanged.source_item_key == item.source_item_key)
            && let Some(previous) = previous_items.get(&item.source_item_key)
            && let Some(artifacts) = artifacts_from_metadata(&previous.metadata)
        {
            put_artifacts(&mut item.metadata, &artifacts);
        }
        item.metadata.insert(
            CACHE_KEY_METADATA_KEY.to_string(),
            serde_json::to_value(DocumentCacheKey {
                source_id: item.source_id.clone(),
                source_item_key: item.source_item_key.clone(),
                generation: Some(manifest.generation.clone()),
            })?,
        );
    }
    ledger.put_manifest(manifest.clone()).await?;
    Ok(())
}

async fn store_artifact(
    runtime: &TargetLocalSourceRuntime,
    kind: ArtifactKind,
    content_type: String,
    content: ContentRef,
    source_id: SourceId,
    job_id: JobId,
    mut metadata: MetadataMap,
) -> anyhow::Result<ArtifactRef> {
    let bytes = content_bytes(&content)?;
    let size_bytes = bytes.len() as u64;
    let content_hash = sha256_prefixed(&bytes);
    metadata.insert(
        "content_hash".to_string(),
        serde_json::json!(content_hash.clone()),
    );
    metadata.insert("size_bytes".to_string(), serde_json::json!(size_bytes));
    let handle = reserved_call::put_artifact(
        runtime,
        ArtifactWriteRequest {
            kind,
            content_type,
            content,
            source_id: Some(source_id),
            job_id: Some(job_id),
            metadata,
        },
    )
    .await?;
    Ok(ArtifactRef {
        artifact_id: handle.artifact_id,
        artifact_kind: handle.artifact_kind,
        uri: handle.uri.unwrap_or_default(),
        size_bytes: Some(size_bytes),
        content_hash: Some(content_hash),
        created_at: timestamp(),
    })
}

fn should_store_artifact(policy: &OutputPolicy, size_bytes: u64) -> bool {
    match policy.artifact_mode {
        ArtifactMode::Always => true,
        ArtifactMode::None => false,
        ArtifactMode::OnLargeOutput => size_bytes > policy.inline_limit_bytes,
    }
}

fn should_inline(policy: &OutputPolicy, size_bytes: u64) -> bool {
    match policy.response_mode {
        ResponseMode::Inline | ResponseMode::Full | ResponseMode::Auto => {
            size_bytes <= policy.inline_limit_bytes
        }
        ResponseMode::Summary
        | ResponseMode::Artifact
        | ResponseMode::Path
        | ResponseMode::JobOnly => false,
    }
}

async fn previous_manifest_items(
    ledger: &dyn axon_ledger::store::LedgerStore,
    diff: &SourceManifestDiff,
) -> anyhow::Result<BTreeMap<SourceItemKey, ManifestItem>> {
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(BTreeMap::new());
    };
    let Some(previous_manifest) = ledger
        .get_manifest(diff.source_id.clone(), previous_generation)
        .await?
    else {
        return Ok(BTreeMap::new());
    };
    Ok(previous_manifest
        .items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect())
}

fn artifacts_from_metadata(metadata: &MetadataMap) -> Option<Vec<ArtifactRef>> {
    metadata
        .get(ARTIFACT_METADATA_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn put_artifacts(metadata: &mut MetadataMap, artifacts: &[ArtifactRef]) {
    if !artifacts.is_empty() {
        metadata.insert(
            ARTIFACT_METADATA_KEY.to_string(),
            serde_json::json!(artifacts),
        );
    }
}

fn content_bytes(content: &ContentRef) -> anyhow::Result<Vec<u8>> {
    match content {
        ContentRef::InlineText { text } => Ok(text.as_bytes().to_vec()),
        ContentRef::InlineBytes { bytes_base64, .. } => {
            Ok(base64::engine::general_purpose::STANDARD.decode(bytes_base64)?)
        }
        ContentRef::Artifact { artifact_id } => Ok(artifact_id.0.as_bytes().to_vec()),
        ContentRef::External { uri, integrity } => Ok(integrity
            .as_deref()
            .unwrap_or(uri.as_str())
            .as_bytes()
            .to_vec()),
    }
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
