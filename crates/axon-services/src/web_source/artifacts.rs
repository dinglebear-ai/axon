use axon_adapters::web;
use axon_api::source::*;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore};
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

use super::WebSourceIndexInput;
use super::run::WebAdapterRun;
use super::run::timestamp;

pub(super) const ARTIFACT_METADATA_KEY: &str = "_axon_artifacts";
pub(super) const CACHE_KEY_METADATA_KEY: &str = "_axon_document_cache_key";

#[derive(Debug, Clone)]
pub(super) struct WebArtifactPayload {
    pub(super) kind: ArtifactKind,
    pub(super) content_type: String,
    pub(super) content: ContentRef,
    pub(super) source_id: SourceId,
    pub(super) job_id: JobId,
    pub(super) metadata: MetadataMap,
    pub(super) content_hash: Option<String>,
    pub(super) size_bytes: Option<u64>,
}

pub(super) async fn store_web_artifact(
    store: &dyn ArtifactStore,
    payload: WebArtifactPayload,
) -> anyhow::Result<ArtifactRef> {
    let bytes = content_bytes(&payload.content)?;
    let size_bytes = payload.size_bytes.unwrap_or(bytes.len() as u64);
    let content_hash = payload
        .content_hash
        .unwrap_or_else(|| sha256_prefixed(&bytes));
    let mut metadata = payload.metadata;
    metadata.insert("producer".to_string(), serde_json::json!("web_source"));
    metadata.insert(
        "source_id".to_string(),
        serde_json::json!(payload.source_id.0.clone()),
    );
    metadata.insert(
        "job_id".to_string(),
        serde_json::json!(payload.job_id.0.to_string()),
    );
    metadata.insert(
        "content_hash".to_string(),
        serde_json::json!(content_hash.clone()),
    );
    metadata.insert("size_bytes".to_string(), serde_json::json!(size_bytes));

    let handle = store
        .put(ArtifactWriteRequest {
            kind: payload.kind,
            content_type: payload.content_type,
            content: payload.content,
            source_id: Some(payload.source_id),
            job_id: Some(payload.job_id),
            metadata,
        })
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

pub(super) async fn store_web_artifact_bytes(
    store: &dyn ArtifactStore,
    payload: WebArtifactPayload,
    bytes: Vec<u8>,
) -> anyhow::Result<ArtifactRef> {
    let size_bytes = payload.size_bytes.unwrap_or(bytes.len() as u64);
    let content_hash = payload
        .content_hash
        .unwrap_or_else(|| sha256_prefixed(&bytes));
    let mut metadata = payload.metadata;
    metadata.insert("producer".to_string(), serde_json::json!("web_source"));
    metadata.insert(
        "source_id".to_string(),
        serde_json::json!(payload.source_id.0.clone()),
    );
    metadata.insert(
        "job_id".to_string(),
        serde_json::json!(payload.job_id.0.to_string()),
    );
    metadata.insert(
        "content_hash".to_string(),
        serde_json::json!(content_hash.clone()),
    );
    metadata.insert("size_bytes".to_string(), serde_json::json!(size_bytes));

    let handle = store
        .put_bytes(ArtifactBytesWriteRequest {
            kind: payload.kind,
            content_type: payload.content_type,
            bytes,
            source_id: Some(payload.source_id),
            job_id: Some(payload.job_id),
            metadata,
        })
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

pub(super) async fn cleanup_artifacts_after_error(
    store: &dyn ArtifactStore,
    artifacts: &[ArtifactRef],
    cause: anyhow::Error,
) -> anyhow::Error {
    let mut cleanup_errors = Vec::new();
    for artifact in artifacts {
        let handle = ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri.clone()),
        };
        if let Err(err) = store.delete(handle).await {
            cleanup_errors.push(format!(
                "failed to delete artifact {} after failed web source generation: {err}",
                artifact.artifact_id.0
            ));
        }
    }
    if cleanup_errors.is_empty() {
        cause
    } else {
        cause.context(cleanup_errors.join("; "))
    }
}

pub(super) fn should_store_artifact(policy: &OutputPolicy, size_bytes: u64) -> bool {
    match policy.artifact_mode {
        ArtifactMode::Always => true,
        ArtifactMode::None => false,
        ArtifactMode::OnLargeOutput => size_bytes > policy.inline_limit_bytes,
    }
}

pub(super) fn should_inline(policy: &OutputPolicy, size_bytes: u64) -> bool {
    match policy.response_mode {
        ResponseMode::Inline | ResponseMode::Full => size_bytes <= policy.inline_limit_bytes,
        ResponseMode::Auto => size_bytes <= policy.inline_limit_bytes,
        ResponseMode::Summary
        | ResponseMode::Artifact
        | ResponseMode::Path
        | ResponseMode::JobOnly => false,
    }
}

pub(super) fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

#[derive(Default)]
pub(super) struct CleanOutput {
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) inline: Option<InlineSourceResult>,
    pub(super) artifact_index: WebArtifactIndex,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct WebArtifactIndex {
    pub(super) generation_artifacts: Vec<ArtifactRef>,
    pub(super) item_artifacts: BTreeMap<SourceItemKey, Vec<ArtifactRef>>,
}

impl WebArtifactIndex {
    pub(super) fn is_empty(&self) -> bool {
        self.generation_artifacts.is_empty() && self.item_artifacts.is_empty()
    }

    pub(super) fn push_generation(&mut self, artifact: ArtifactRef) {
        self.generation_artifacts.push(artifact);
    }

    pub(super) fn push_item(&mut self, source_item_key: SourceItemKey, artifact: ArtifactRef) {
        self.item_artifacts
            .entry(source_item_key)
            .or_default()
            .push(artifact);
    }

    pub(super) fn merge(&mut self, other: WebArtifactIndex) {
        self.generation_artifacts.extend(other.generation_artifacts);
        for (source_item_key, artifacts) in other.item_artifacts {
            self.item_artifacts
                .entry(source_item_key)
                .or_default()
                .extend(artifacts);
        }
    }
}

pub(super) async fn record_artifacts_on_manifest(
    ledger: &dyn axon_ledger::store::LedgerStore,
    manifest: &mut SourceManifest,
    diff: &SourceManifestDiff,
    artifact_index: &WebArtifactIndex,
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
        let cache_key = DocumentCacheKey {
            source_id: item.source_id.clone(),
            source_item_key: item.source_item_key.clone(),
            generation: Some(manifest.generation.clone()),
        };
        item.metadata.insert(
            CACHE_KEY_METADATA_KEY.to_string(),
            serde_json::to_value(cache_key)?,
        );
    }
    ledger.put_manifest(manifest.clone()).await?;
    Ok(())
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
    if artifacts.is_empty() {
        return;
    }
    metadata.insert(
        ARTIFACT_METADATA_KEY.to_string(),
        serde_json::json!(artifacts),
    );
}

pub(super) async fn store_warc_artifact(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    items: &[AcquiredSourceItem],
) -> anyhow::Result<Vec<ArtifactRef>> {
    if input.output.artifact_mode == ArtifactMode::None
        || !run
            .plan
            .route
            .validated_options
            .values
            .contains_key("warc_path")
        || items.is_empty()
    {
        return Ok(Vec::new());
    }
    let archive = web::build_warc_archive(items);
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "artifact_role".to_string(),
        serde_json::json!("web_warc_archive"),
    );
    let artifact = store_web_artifact_bytes(
        input.artifact_store.as_ref(),
        WebArtifactPayload {
            kind: ArtifactKind::Warc,
            content_type: "application/warc".to_string(),
            content: ContentRef::External {
                uri: "warc://generated".to_string(),
                integrity: Some(archive.sha256.clone()),
            },
            source_id: run.source_id.clone(),
            job_id: input.job_id,
            metadata,
            content_hash: Some(archive.sha256),
            size_bytes: Some(archive.size_bytes),
        },
        archive.bytes,
    )
    .await?;
    Ok(vec![artifact])
}

pub(super) async fn store_clean_outputs(
    input: &WebSourceIndexInput,
    documents: &[SourceDocument],
) -> anyhow::Result<CleanOutput> {
    let mut output = CleanOutput::default();
    for document in documents {
        let bytes = content_bytes(&document.content)?;
        let size_bytes = bytes.len() as u64;
        if output.inline.is_none() && should_inline(&input.output, size_bytes) {
            output.inline = Some(InlineSourceResult {
                content: Some(document.content.clone()),
                summary: None,
                metadata: document.metadata.clone(),
            });
        }
        if should_store_artifact(&input.output, size_bytes) {
            let mut metadata = document.metadata.clone();
            metadata.insert(
                "source_item_key".to_string(),
                serde_json::json!(document.source_item_key.0.clone()),
            );
            metadata.insert(
                "canonical_uri".to_string(),
                serde_json::json!(document.canonical_uri.clone()),
            );
            let artifact = store_web_artifact(
                input.artifact_store.as_ref(),
                WebArtifactPayload {
                    kind: ArtifactKind::NormalizedContent,
                    content_type: document
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "text/markdown".to_string()),
                    content: document.content.clone(),
                    source_id: document.source_id.clone(),
                    job_id: input.job_id,
                    metadata,
                    content_hash: Some(sha256_prefixed(&bytes)),
                    size_bytes: Some(size_bytes),
                },
            )
            .await?;
            output
                .artifact_index
                .push_item(document.source_item_key.clone(), artifact.clone());
            output.artifacts.push(artifact);
        }
    }
    Ok(output)
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
