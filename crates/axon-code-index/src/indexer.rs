use std::path::Path;

use crate::config::{CodeIndexIdentity, max_indexed_file_bytes};
use crate::manifest::{FileDiff, FileManifestEntry, ManifestSnapshot};
use crate::paths::code_path_prefixes;
use crate::progress::{ReindexProgress, ReindexProgressSink, ReindexRunOptions, emit_progress};
use crate::store::CodeIndexStore;
use crate::summary::ReindexSummary;
use axon_core::config::Config;
use axon_vector::ops::{
    SourceDocument, SourceOrigin, embed_prepared_docs, prepare_source_document,
};

pub(crate) async fn reindex_changed_files(
    cfg: &Config,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
    progress: Option<&dyn ReindexProgressSink>,
) -> anyhow::Result<ReindexSummary> {
    let generation = store.next_generation(identity).await?;
    reindex_changed_files_inner(
        Some(cfg),
        store,
        identity,
        manifest,
        diff,
        ReindexRunOptions {
            generation,
            progress,
        },
    )
    .await
}

pub(crate) async fn finish_completed_generation(
    _cfg: &Config,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    progress: Option<&dyn ReindexProgressSink>,
) -> anyhow::Result<ReindexSummary> {
    finish_completed_generation_inner(store, identity, manifest, generation, progress).await
}

async fn reindex_changed_files_inner(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
    options: ReindexRunOptions<'_>,
) -> anyhow::Result<ReindexSummary> {
    let mut summary = ReindexSummary::default();
    let generation = options.generation;
    let progress = options.progress;
    let previous_generation = generation.saturating_sub(1);
    let mut cleanup_paths = diff.removed.clone();
    let batch_size = axon_core::config::parse::tuning::code_search_changed_file_batch_size();
    let total_batches = manifest.files.len().div_ceil(batch_size);
    emit_progress(
        progress,
        ReindexProgress::Started {
            generation,
            total_files: manifest.files.len(),
            added_files: diff.added.len(),
            modified_files: diff.modified.len(),
            removed_files: diff.removed.len(),
            total_batches,
        },
    );
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
        progress,
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
    emit_progress(
        progress,
        ReindexProgress::CleanupStarted {
            generation,
            cleanup_paths: cleanup_paths.len(),
        },
    );
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex commit start"
    );
    emit_progress(progress, ReindexProgress::CommitStarted { generation });
    store.commit_generation(identity, generation).await?;
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex commit done"
    );
    tracing::info!(
        project_key = %identity.project_key,
        generation,
        "code-search reindex cleanup done"
    );
    emit_progress(progress, ReindexProgress::Finished { generation });
    Ok(summary)
}

async fn reindex_manifest_batches(
    cfg: Option<&Config>,
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    progress: Option<&dyn ReindexProgressSink>,
    cleanup_paths: &mut Vec<String>,
) -> anyhow::Result<()> {
    let batch_size = axon_core::config::parse::tuning::code_search_changed_file_batch_size();
    let total_batches = manifest.files.len().div_ceil(batch_size);
    for (batch_index, batch) in manifest.files.chunks(batch_size).enumerate() {
        let batch_number = batch_index + 1;
        log_reindex_batch_start(identity, generation, batch_number, total_batches, batch);
        let prepared =
            prepare_reindex_batch(store, identity, generation, cleanup_paths, batch).await?;
        let embedded_docs = prepared.len();
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
        let processed_files = (batch_number * batch_size).min(manifest.files.len());
        emit_progress(
            progress,
            ReindexProgress::BatchFinished {
                generation,
                batch_number,
                total_batches,
                processed_files,
                total_files: manifest.files.len(),
                batch_files: batch.len(),
                embedded_docs,
            },
        );
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

async fn finish_completed_generation_inner(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
    progress: Option<&dyn ReindexProgressSink>,
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
    emit_progress(
        progress,
        ReindexProgress::CleanupStarted {
            generation,
            cleanup_paths: cleanup_paths.len(),
        },
    );
    emit_progress(progress, ReindexProgress::CommitStarted { generation });
    store.commit_generation(identity, generation).await?;
    emit_progress(progress, ReindexProgress::Finished { generation });
    Ok(ReindexSummary::default())
}

#[cfg(test)]
pub(crate) async fn reindex_changed_files_for_test(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    diff: &FileDiff,
    generation: i64,
) -> anyhow::Result<ReindexSummary> {
    reindex_changed_files_inner(
        None,
        store,
        identity,
        manifest,
        diff,
        ReindexRunOptions {
            generation,
            progress: None,
        },
    )
    .await
}

#[cfg(test)]
pub(crate) async fn finish_completed_generation_for_test(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    manifest: &ManifestSnapshot,
    generation: i64,
) -> anyhow::Result<ReindexSummary> {
    finish_completed_generation_inner(store, identity, manifest, generation, None).await
}
