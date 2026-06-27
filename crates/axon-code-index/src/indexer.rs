use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::config::{CodeIndexIdentity, max_indexed_file_bytes};
use crate::manifest::{FileDiff, FileManifestEntry, ManifestSnapshot};
use crate::store::CodeIndexStore;
use axon_core::config::Config;
use axon_vector::ops::{
    SourceDocument, SourceOrigin, embed_prepared_docs, prepare_source_document,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReindexSummary {
    pub indexed_files: usize,
    pub removed_files: usize,
}

pub(crate) async fn reindex_changed_files(
    cfg: &Config,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
) -> anyhow::Result<ReindexSummary> {
    let generation = store.next_generation(identity).await?;
    reindex_changed_files_inner(Some(cfg), store, identity, manifest, diff, generation, None).await
}

pub(crate) async fn retry_cleanup_debt(
    cfg: &Config,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
) -> anyhow::Result<()> {
    cleanup_debt(Some(cfg), store, identity, None).await
}

pub(crate) async fn finish_completed_generation(
    cfg: &Config,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
) -> anyhow::Result<ReindexSummary> {
    finish_completed_generation_inner(Some(cfg), store, identity, manifest, generation, None).await
}

async fn reindex_changed_files_inner(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
    generation: i64,
    test_deletes: Option<Arc<Mutex<Vec<String>>>>,
) -> anyhow::Result<ReindexSummary> {
    let mut summary = ReindexSummary::default();
    let previous_generation = generation.saturating_sub(1);
    let mut cleanup_paths = diff.removed.clone();
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        manifest_files = manifest.files.len(),
        diff_added = diff.added.len(),
        diff_modified = diff.modified.len(),
        diff_removed = diff.removed.len(),
        "code-search reindex start"
    );

    for removed in &diff.removed {
        store.remove_file(identity, removed).await?;
        summary.removed_files += 1;
    }

    reindex_manifest_batches(
        cfg,
        store,
        identity,
        manifest,
        generation,
        &mut cleanup_paths,
    )
    .await?;
    summary.indexed_files = diff.added.len() + diff.modified.len();

    cleanup_paths.sort();
    cleanup_paths.dedup();
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        cleanup_paths = cleanup_paths.len(),
        previous_generation,
        "code-search reindex cleanup debt start"
    );
    store
        .add_cleanup_debt(identity, previous_generation, &cleanup_paths)
        .await?;
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex commit start"
    );
    store.commit_generation(identity, generation).await?;
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex commit done"
    );
    cleanup_debt(cfg, store, identity, test_deletes).await?;
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex cleanup done"
    );
    Ok(summary)
}

async fn reindex_manifest_batches(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    cleanup_paths: &mut Vec<String>,
) -> anyhow::Result<()> {
    let batch_size = changed_file_batch_size();
    let total_batches = manifest.files.len().div_ceil(batch_size);
    for (batch_index, batch) in manifest.files.chunks(batch_size).enumerate() {
        let batch_number = batch_index + 1;
        log_reindex_batch_start(identity, generation, batch_number, total_batches, batch);
        let prepared =
            prepare_reindex_batch(store, identity, generation, cleanup_paths, batch).await?;
        embed_reindex_batch(
            cfg,
            identity,
            generation,
            batch_number,
            total_batches,
            prepared,
        )
        .await?;
        mark_reindex_batch_indexed(
            store,
            identity,
            generation,
            batch_number,
            total_batches,
            batch,
        )
        .await?;
    }
    Ok(())
}

fn log_reindex_batch_start(
    identity: &CodeIndexIdentity,
    generation: i64,
    batch_number: usize,
    total_batches: usize,
    batch: &[FileManifestEntry],
) {
    let first_path = batch
        .first()
        .map(|entry| entry.relative_path.as_str())
        .unwrap_or_default();
    let last_path = batch
        .last()
        .map(|entry| entry.relative_path.as_str())
        .unwrap_or_default();
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        batch = batch_number,
        total_batches,
        batch_files = batch.len(),
        first_path,
        last_path,
        "code-search reindex batch start"
    );
}

async fn prepare_reindex_batch(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    generation: i64,
    cleanup_paths: &mut Vec<String>,
    batch: &[FileManifestEntry],
) -> anyhow::Result<Vec<axon_vector::ops::PreparedDoc>> {
    let mut prepared = Vec::new();
    for entry in batch {
        store
            .mark_file_pending(identity, &entry.relative_path)
            .await?;
        cleanup_paths.push(entry.relative_path.clone());
        if entry.size_bytes == 0 {
            continue;
        }
        if let Some(doc) = prepare_local_code_doc(identity, entry, generation).await? {
            prepared.push(doc);
        }
    }
    Ok(prepared)
}

async fn embed_reindex_batch(
    cfg: Option<&Config>,
    identity: &CodeIndexIdentity,
    generation: i64,
    batch_number: usize,
    total_batches: usize,
    prepared: Vec<axon_vector::ops::PreparedDoc>,
) -> anyhow::Result<()> {
    let Some(cfg) = cfg else {
        return Ok(());
    };
    if prepared.is_empty() {
        return Ok(());
    }
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        batch = batch_number,
        total_batches,
        prepared_docs = prepared.len(),
        "code-search reindex batch embed start"
    );
    embed_prepared_docs(cfg, prepared, None)
        .await
        .map_err(|err| anyhow::anyhow!("code search embed failed: {err}"))?
        .require_success("code search embed")
        .map_err(|err| anyhow::anyhow!(err))?;
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        batch = batch_number,
        total_batches,
        "code-search reindex batch embed done"
    );
    Ok(())
}

async fn mark_reindex_batch_indexed(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    generation: i64,
    batch_number: usize,
    total_batches: usize,
    batch: &[FileManifestEntry],
) -> anyhow::Result<()> {
    for entry in batch {
        store.mark_file_indexed(identity, entry, generation).await?;
    }
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        batch = batch_number,
        total_batches,
        "code-search reindex batch marked indexed"
    );
    Ok(())
}

async fn prepare_local_code_doc(
    identity: &CodeIndexIdentity,
    entry: &FileManifestEntry,
    generation: i64,
) -> anyhow::Result<Option<axon_vector::ops::PreparedDoc>> {
    let path = identity.project_root.join(&entry.relative_path);
    let metadata = tokio::fs::metadata(&path).await?;
    if metadata.len() > max_indexed_file_bytes() {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&path).await?;
    let content = match String::from_utf8(bytes) {
        Ok(content) if !content.trim().is_empty() => content,
        _ => return Ok(None),
    };
    let ext = Path::new(&entry.relative_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extra = serde_json::json!({
        "source_type": "local_code",
        "local_project_key": identity.project_key,
        "local_project_display": identity.project_display,
        "local_file_hash": entry.hash.as_deref().unwrap_or_default(),
        "local_index_version": identity.index_version,
        "local_generation": generation,
        "code_file_path": entry.relative_path,
        "code_path_prefixes": code_path_prefixes(&entry.relative_path),
    });
    let url = format!(
        "local-code://{}/g/{}/{}",
        identity.project_key,
        generation,
        percent_encoding::utf8_percent_encode(
            &entry.relative_path,
            percent_encoding::NON_ALPHANUMERIC
        )
    );
    let source = SourceDocument::try_new_file(
        SourceOrigin::LocalFile,
        url,
        entry.relative_path.clone(),
        ext,
        content,
        "local_code",
        Some(entry.relative_path.clone()),
        Some(extra),
    )
    .map_err(|err| anyhow::anyhow!("invalid local code source document: {err}"))?;
    prepare_source_document(source)
        .await
        .map(Some)
        .map_err(|err| anyhow::anyhow!("prepare local code source document failed: {err}"))
}

pub(crate) fn code_path_prefixes(relative_path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    let mut current = String::new();
    let parts = relative_path.split('/').collect::<Vec<_>>();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        if part.is_empty() {
            continue;
        }
        current.push_str(part);
        current.push('/');
        prefixes.push(current.clone());
    }
    prefixes
}

async fn cleanup_debt(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    test_deletes: Option<Arc<Mutex<Vec<String>>>>,
) -> anyhow::Result<()> {
    let debt = store.cleanup_debt(identity).await?;
    for (generation, paths) in debt {
        if generation <= 0 || paths.is_empty() {
            store
                .clear_cleanup_debt(identity, generation, &paths)
                .await?;
            continue;
        }
        if let Some(cfg) = cfg {
            axon_vector::ops::qdrant::qdrant_delete_local_code_files_for_generation(
                cfg,
                &identity.project_key,
                generation,
                &paths,
            )
            .await?;
        }
        if let Some(deletes) = &test_deletes {
            deletes
                .lock()
                .map_err(|err| anyhow::anyhow!("cleanup delete tracker lock poisoned: {err}"))?
                .extend(paths.iter().cloned());
        }
        store
            .clear_cleanup_debt(identity, generation, &paths)
            .await?;
    }
    Ok(())
}

async fn finish_completed_generation_inner(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    test_deletes: Option<Arc<Mutex<Vec<String>>>>,
) -> anyhow::Result<ReindexSummary> {
    let previous_generation = generation.saturating_sub(1);
    let cleanup_paths = manifest
        .files
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect::<Vec<_>>();
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        cleanup_paths = cleanup_paths.len(),
        previous_generation,
        "code-search finish completed generation"
    );
    store
        .add_cleanup_debt(identity, previous_generation, &cleanup_paths)
        .await?;
    store.commit_generation(identity, generation).await?;
    cleanup_debt(cfg, store, identity, test_deletes).await?;
    Ok(ReindexSummary::default())
}

fn changed_file_batch_size() -> usize {
    axon_core::config::parse::tuning::code_search_changed_file_batch_size()
}

#[cfg(test)]
pub(crate) async fn reindex_changed_files_for_test(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
    generation: i64,
    deletes: Arc<Mutex<Vec<String>>>,
) -> anyhow::Result<ReindexSummary> {
    reindex_changed_files_inner(
        None,
        store,
        identity,
        manifest,
        diff,
        generation,
        Some(deletes),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn finish_completed_generation_for_test(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    deletes: Arc<Mutex<Vec<String>>>,
) -> anyhow::Result<ReindexSummary> {
    finish_completed_generation_inner(None, store, identity, manifest, generation, Some(deletes))
        .await
}
