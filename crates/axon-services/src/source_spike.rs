use axon_source_ledger::{
    ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore, SourceStatus,
};
use axon_vector::ops::file_ingest::{SelectionPolicy, collect_files};
use axon_vector::ops::input::classify::path_extension;
use axon_vector::ops::{
    LedgerPayload, PreparedDoc, SourceDocument, SourceOrigin, prepare_source_document,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use url::Url;

const LOCAL_SOURCE_SPIKE_INDEX_VERSION: i64 = 1;
const LOCAL_SOURCE_SPIKE_LEASE_TTL_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone)]
pub(crate) struct LocalSourceSpikeInput {
    pub(crate) root: PathBuf,
    pub(crate) collection: String,
    pub(crate) owner: String,
}

#[derive(Debug)]
pub(crate) struct LocalSourceSpikeOutput {
    pub(crate) source_id: String,
    pub(crate) source_kind: SourceKind,
    pub(crate) collection: String,
    pub(crate) generation: i64,
    pub(crate) manifest_items: Vec<LocalSourceSpikeManifestItem>,
    pub(crate) prepared_docs: Vec<PreparedDoc>,
    pub(crate) cleanup_placeholders: Vec<LocalSourceCleanupPlaceholder>,
    pub(crate) status: SourceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalSourceSpikeManifestItem {
    pub(crate) item_key: String,
    pub(crate) content_hash: String,
    pub(crate) size_bytes: i64,
    pub(crate) modified_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalSourceCleanupPlaceholder {
    pub(crate) source_id: String,
    pub(crate) generation: i64,
    pub(crate) executed: bool,
}

#[derive(Debug, Clone)]
struct LocalSourceItem {
    path: PathBuf,
    item_key: String,
    manifest_item: ManifestItem,
    output_item: LocalSourceSpikeManifestItem,
    extension: String,
    file_url: String,
    text: String,
}

pub(crate) async fn prepare_local_source_spike(
    input: LocalSourceSpikeInput,
    store: &SourceLedgerStore,
) -> anyhow::Result<LocalSourceSpikeOutput> {
    let root = canonicalize_existing_path(&input.root).await?;
    let source = SourceIdentity::new(
        local_source_id(&root)?,
        SourceKind::LocalCode,
        input.collection.clone(),
        LOCAL_SOURCE_SPIKE_INDEX_VERSION,
    );
    if !store
        .acquire_lease(&source, &input.owner, LOCAL_SOURCE_SPIKE_LEASE_TTL_MS)
        .await?
    {
        anyhow::bail!(
            "source ledger refresh already running for {}",
            source.source_id
        );
    }

    let mut active_generation = None;
    let result: anyhow::Result<LocalSourceSpikeOutput> = async {
        store.ensure_source(&source).await?;
        let items = discover_local_items(&root).await?;
        let ledger_manifest_items = items
            .iter()
            .map(|item| item.manifest_item.clone())
            .collect::<Vec<_>>();
        let manifest_items = items
            .iter()
            .map(|item| item.output_item.clone())
            .collect::<Vec<_>>();
        let _diff = store
            .diff_manifest(&source.source_id, &ledger_manifest_items)
            .await?;
        let generation = store
            .begin_generation_for_owner(&source, &input.owner)
            .await?;
        active_generation = Some(generation);

        for item in &ledger_manifest_items {
            store
                .record_manifest_item(&source.source_id, generation, item.clone())
                .await?;
        }

        let mut prepared_docs = Vec::with_capacity(items.len());
        for item in items {
            let source_doc = source_document_for_item(&source, generation, &item)?;
            let prepared = prepare_source_document(source_doc)
                .await
                .map_err(|err| anyhow::anyhow!("failed to prepare {}: {err}", item.item_key))?;
            prepared_docs.push(prepared);
        }

        let status = store.source_status(&source.source_id).await?;
        store.release_lease(&source.source_id, &input.owner).await?;
        Ok(LocalSourceSpikeOutput {
            source_id: source.source_id.clone(),
            source_kind: source.source_kind.clone(),
            collection: source.collection.clone(),
            generation,
            manifest_items,
            prepared_docs,
            cleanup_placeholders: vec![LocalSourceCleanupPlaceholder {
                source_id: source.source_id.clone(),
                generation,
                executed: false,
            }],
            status,
        })
    }
    .await;

    match result {
        Ok(output) => Ok(output),
        Err(err) => {
            if let Some(generation) = active_generation {
                let abort_result = store
                    .abort_generation_for_owner(&source.source_id, generation, &input.owner)
                    .await;
                let release_result = store.release_lease(&source.source_id, &input.owner).await;
                match (abort_result, release_result) {
                    (Ok(()), Ok(())) => Err(err),
                    (Err(abort_err), Ok(())) => Err(err.context(format!(
                        "additionally failed to abort source generation: {abort_err}"
                    ))),
                    (Ok(()), Err(release_err)) => Err(err.context(format!(
                        "additionally failed to release source lease: {release_err}"
                    ))),
                    (Err(abort_err), Err(release_err)) => Err(err.context(format!(
                        "additionally failed to abort source generation: {abort_err}; \
                         additionally failed to release source lease: {release_err}"
                    ))),
                }
            } else {
                let release_result = store.release_lease(&source.source_id, &input.owner).await;
                match release_result {
                    Ok(()) => Err(err),
                    Err(release_err) => Err(err.context(format!(
                        "additionally failed to release source lease: {release_err}"
                    ))),
                }
            }
        }
    }
}

async fn canonicalize_existing_path(path: &Path) -> anyhow::Result<PathBuf> {
    tokio::fs::canonicalize(path)
        .await
        .map_err(|err| anyhow::anyhow!("invalid local source root {}: {err}", path.display()))
}

async fn discover_local_items(root: &Path) -> anyhow::Result<Vec<LocalSourceItem>> {
    let metadata = tokio::fs::metadata(root).await.map_err(|err| {
        anyhow::anyhow!("failed to stat local source root {}: {err}", root.display())
    })?;
    let paths = if metadata.is_file() {
        vec![root.to_path_buf()]
    } else if metadata.is_dir() {
        collect_files(root, SelectionPolicy::Permissive).await?
    } else {
        anyhow::bail!(
            "local source root {} is not a file or directory",
            root.display()
        );
    };
    let mut items = Vec::with_capacity(paths.len());
    for path in paths {
        items.push(local_source_item(root, &path, metadata.is_file()).await?);
    }
    items.sort_by(|a, b| a.item_key.cmp(&b.item_key));
    Ok(items)
}

async fn local_source_item(
    root: &Path,
    path: &Path,
    root_is_file: bool,
) -> anyhow::Result<LocalSourceItem> {
    let metadata = tokio::fs::metadata(path).await.map_err(|err| {
        anyhow::anyhow!("failed to stat local source file {}: {err}", path.display())
    })?;
    let bytes = tokio::fs::read(path).await.map_err(|err| {
        anyhow::anyhow!("failed to read local source file {}: {err}", path.display())
    })?;
    let item_key = item_key_for_path(root, path, root_is_file)?;
    let content_hash = content_hash(&bytes);
    let size_bytes = i64::try_from(bytes.len())
        .map_err(|_| anyhow::anyhow!("file {} is too large for source ledger", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|err| {
        anyhow::anyhow!(
            "local source file {} is not valid UTF-8: {err}",
            path.display()
        )
    })?;
    let extension = path_extension(&item_key).to_ascii_lowercase();
    let file_url = file_url_for_path(path)?;
    let modified_at_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    Ok(LocalSourceItem {
        path: path.to_path_buf(),
        item_key: item_key.clone(),
        manifest_item: ManifestItem::new(item_key.clone(), content_hash.clone(), size_bytes),
        output_item: LocalSourceSpikeManifestItem {
            item_key,
            content_hash,
            size_bytes,
            modified_at_ms,
        },
        extension,
        file_url,
        text,
    })
}

fn source_document_for_item(
    source: &SourceIdentity,
    generation: i64,
    item: &LocalSourceItem,
) -> anyhow::Result<SourceDocument> {
    let title = Some(item.item_key.clone());
    let extra = Some(serde_json::json!({
        "local_file_path": item.item_key,
        "local_file_extension": item.extension,
        "local_file_hash": item.output_item.content_hash,
        "local_file_size_bytes": item.output_item.size_bytes,
        "local_file_modified_at_ms": item.output_item.modified_at_ms,
        "local_content_kind": local_content_kind(&item.extension),
    }));
    let payload = LedgerPayload::try_new(
        source.source_id.clone(),
        source.source_kind.as_str(),
        generation,
        item.item_key.clone(),
        source.index_version,
    )
    .map_err(|err| anyhow::anyhow!("invalid local ledger payload: {err}"))?;
    let doc = SourceDocument::try_new_file(
        SourceOrigin::LocalFile,
        item.file_url.clone(),
        item.item_key.clone(),
        item.extension.clone(),
        item.text.clone(),
        source.source_kind.as_str(),
        title,
        extra,
    )
    .map_err(|err| anyhow::anyhow!("invalid local file source document: {err}"))?;
    Ok(doc.with_ledger_payload(payload))
}

fn local_source_id(root: &Path) -> anyhow::Result<String> {
    Ok(format!("local:{}", file_url_for_path(root)?))
}

fn file_url_for_path(path: &Path) -> anyhow::Result<String> {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|()| anyhow::anyhow!("failed to build file URL for {}", path.display()))
}

fn item_key_for_path(root: &Path, path: &Path, root_is_file: bool) -> anyhow::Result<String> {
    let relative = if root_is_file {
        path.file_name()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("file path {} has no file name", path.display()))?
    } else {
        path.strip_prefix(root)
            .map_err(|err| {
                anyhow::anyhow!(
                    "failed to relativize {} under {}: {err}",
                    path.display(),
                    root.display()
                )
            })?
            .to_path_buf()
    };
    let key = relative
        .to_str()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "local source item key must be valid UTF-8 for {}",
                path.display()
            )
        })?
        .replace('\\', "/");
    if key.is_empty() {
        anyhow::bail!(
            "local source item key cannot be empty for {}",
            path.display()
        );
    }
    Ok(key)
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn local_content_kind(extension: &str) -> &'static str {
    match extension {
        "md" | "mdx" | "rst" => "markdown",
        "txt" | "text" => "plain_text",
        _ => "code_or_config",
    }
}

#[cfg(test)]
#[path = "source_spike_tests.rs"]
mod tests;
