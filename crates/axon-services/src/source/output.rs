//! Shared source output and durable artifact publication.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use axon_adapters::SourceAdapter;
use axon_api::source::*;
use axon_core::boundary::ArtifactBytesWriteRequest;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;

use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ProviderCallContext};

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

#[derive(serde::Serialize)]
struct DurableManifestEntry<'a> {
    url: &'a str,
    relative_path: String,
    markdown_chars: usize,
    content_hash: String,
    changed: bool,
    source_item_key: &'a str,
    document_id: &'a str,
}

/// Prepare the caller-requested filesystem export before processing a new
/// generation. The manifest is deliberately created before acquisition so it
/// remains a usable (possibly empty) checkpoint when later pipeline stages
/// fail. Existing markdown files are content-addressed by URL and overwritten
/// atomically as their documents complete; truncating only the manifest avoids
/// a destructive directory-wide reset.
pub(crate) async fn initialize_durable_export(plan: &SourcePlan) -> anyhow::Result<()> {
    let Some(output_dir) = durable_output_dir(plan) else {
        return Ok(());
    };
    initialize_durable_export_dir(&output_dir).await
}

async fn initialize_durable_export_dir(output_dir: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(output_dir.join("markdown")).await?;
    sync_replace(&output_dir.join("manifest.jsonl"), b"").await
}

/// Materialize normalized documents independently of generation publication.
/// Each content file is atomically renamed into place, then its JSONL record is
/// appended and `sync_data`'d. Consequently every visible manifest line names
/// a fully written document, even if the process exits or loses its lease
/// before the source generation is published.
pub(crate) async fn checkpoint_durable_export(
    plan: &SourcePlan,
    documents: &[SourceDocument],
) -> anyhow::Result<()> {
    let Some(output_dir) = durable_output_dir(plan) else {
        return Ok(());
    };
    checkpoint_durable_export_dir(&output_dir, documents).await
}

async fn checkpoint_durable_export_dir(
    output_dir: &Path,
    documents: &[SourceDocument],
) -> anyhow::Result<()> {
    let markdown_dir = output_dir.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir).await?;
    let manifest_path = output_dir.join("manifest.jsonl");
    let mut manifest = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest_path)
        .await?;

    for document in documents {
        let bytes = content_bytes(&document.content)?;
        let filename = axon_core::content::url_to_stable_filename(&document.canonical_uri);
        let relative_path = Path::new("markdown").join(&filename);
        sync_replace(&markdown_dir.join(filename), &bytes).await?;
        let entry = DurableManifestEntry {
            url: &document.canonical_uri,
            relative_path: relative_path.to_string_lossy().into_owned(),
            markdown_chars: String::from_utf8_lossy(&bytes).chars().count(),
            content_hash: sha256_prefixed(&bytes),
            changed: true,
            source_item_key: &document.source_item_key.0,
            document_id: &document.document_id.0,
        };
        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        manifest.write_all(&line).await?;
        manifest.sync_data().await?;
    }
    Ok(())
}

fn durable_output_dir(plan: &SourcePlan) -> Option<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("output_dir")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

async fn sync_replace(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("output path has no parent: {}", path.display()))?;
    tokio::fs::create_dir_all(parent).await?;
    let temp = tempfile::Builder::new()
        .prefix(".axon-checkpoint-")
        .tempfile_in(parent)?;
    let temp_path = temp.into_temp_path();
    let mut file = tokio::fs::File::create(&temp_path).await?;
    file.write_all(bytes).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&temp_path, path).await?;
    Ok(())
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
    checkpoint_durable_export(plan, documents).await?;
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
            &document.content,
            document.source_id.clone(),
            plan,
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
        artifact_call_context(plan, format!("archive:{}", archive.content_hash)),
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
    let unchanged_keys = diff
        .unchanged
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect::<BTreeSet<_>>();
    let previous_items = previous_manifest_items(ledger, diff, &unchanged_keys).await?;
    put_artifacts(&mut manifest.metadata, &artifact_index.generation_artifacts);
    for item in &mut manifest.items {
        if let Some(artifacts) = artifact_index.item_artifacts.get(&item.source_item_key) {
            put_artifacts(&mut item.metadata, artifacts);
        } else if unchanged_keys.contains(&item.source_item_key)
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
    ledger.put_manifest_ref(manifest).await?;
    Ok(())
}

fn artifact_call_context(
    plan: &SourcePlan,
    operation_id: impl Into<String>,
) -> ProviderCallContext {
    let (attempt, priority) = read_provider_execution_metadata(&plan.request.metadata)
        .map(|execution| (execution.attempt, execution.priority))
        .unwrap_or((1, plan.request.execution.priority));
    ProviderCallContext::for_phase(
        plan.job_id,
        attempt,
        PipelinePhase::Publishing,
        priority,
        operation_id,
    )
}

async fn store_artifact(
    runtime: &TargetLocalSourceRuntime,
    kind: ArtifactKind,
    content_type: String,
    content: &ContentRef,
    source_id: SourceId,
    plan: &SourcePlan,
    mut metadata: MetadataMap,
) -> anyhow::Result<ArtifactRef> {
    let bytes = content_bytes(content)?;
    let size_bytes = bytes.len() as u64;
    let content_hash = sha256_prefixed(&bytes);
    metadata.insert(
        "content_hash".to_string(),
        serde_json::json!(content_hash.clone()),
    );
    metadata.insert("size_bytes".to_string(), serde_json::json!(size_bytes));
    let handle = reserved_call::put_artifact(
        runtime,
        artifact_call_context(plan, format!("artifact:{content_hash}")),
        ArtifactWriteRequest {
            kind,
            content_type,
            content: (*content).clone(),
            source_id: Some(source_id),
            job_id: Some(plan.job_id),
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
    wanted: &BTreeSet<SourceItemKey>,
) -> anyhow::Result<BTreeMap<SourceItemKey, ManifestItem>> {
    let Some(previous_generation) = diff.previous_generation.clone() else {
        return Ok(BTreeMap::new());
    };
    if wanted.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(ledger
        .get_manifest_items_with_metadata_key(
            diff.source_id.clone(),
            previous_generation,
            wanted.iter().cloned().collect(),
            ARTIFACT_METADATA_KEY.to_string(),
        )
        .await?
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

fn content_bytes(content: &ContentRef) -> anyhow::Result<Cow<'_, [u8]>> {
    match content {
        ContentRef::InlineText { text } => Ok(Cow::Borrowed(text.as_bytes())),
        ContentRef::InlineBytes { bytes_base64, .. } => Ok(Cow::Owned(
            base64::engine::general_purpose::STANDARD.decode(bytes_base64)?,
        )),
        ContentRef::Artifact { artifact_id } => Ok(Cow::Borrowed(artifact_id.0.as_bytes())),
        ContentRef::External { uri, integrity } => Ok(Cow::Borrowed(
            integrity.as_deref().unwrap_or(uri.as_str()).as_bytes(),
        )),
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
