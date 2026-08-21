//! Session-file discovery, bounded hashing, and manifest construction.

use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder, WalkState};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::adapter::Result;
use crate::manifest::item_identity;

use super::project_filter::matches_project_filter;
use super::target::SessionTarget;
use super::{
    SESSION_DISCOVERY_HASH_MAX_THREADS, SESSION_DOCUMENT_VERSION, fs_error, hex_prefix, modified_at,
};

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct SessionFileCandidate {
    key: String,
}

fn session_walk_builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(false)
        .git_exclude(false)
        .parents(false)
        .filter_entry(should_descend_entry);
    builder
}

fn session_hash_threads(work_items: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    cpus.div_ceil(2)
        .clamp(1, SESSION_DISCOVERY_HASH_MAX_THREADS)
        .min(work_items.max(1))
}

pub(super) fn collect_capped_session_candidates(
    target: &SessionTarget,
    project_filter: Option<&str>,
    root: &Path,
    limit: usize,
) -> Result<Vec<SessionFileCandidate>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut selected = BinaryHeap::with_capacity(limit.min(4096));
    for entry in session_walk_builder(root).build() {
        let entry = entry.map_err(session_walk_error)?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.into_path();
        if !target_has_supported_session_extension(target, &path) {
            continue;
        }
        let key = relative_key(root, &path)?;
        if !matches_project_filter(project_filter, root, &path, &key) {
            continue;
        }
        let candidate = SessionFileCandidate { key };
        if selected.len() < limit {
            selected.push(candidate);
        } else if selected.peek().is_some_and(|largest| candidate < *largest) {
            selected.pop();
            selected.push(candidate);
        }
    }
    let mut selected = selected.into_vec();
    selected.sort();
    Ok(selected)
}

pub(super) fn hash_session_candidates_parallel(
    plan: &SourcePlan,
    base_uri: &str,
    root: &Path,
    candidates: &[SessionFileCandidate],
) -> Result<Vec<ManifestItem>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let threads = session_hash_threads(candidates.len());
    let chunk_size = candidates.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for chunk in candidates.chunks(chunk_size) {
            handles.push(scope.spawn(move || -> Result<Vec<ManifestItem>> {
                let mut items = Vec::with_capacity(chunk.len());
                for candidate in chunk {
                    if let Some(item) = session_manifest_item(plan, base_uri, root, &candidate.key)?
                    {
                        items.push(item);
                    }
                }
                Ok(items)
            }));
        }
        let mut items = Vec::with_capacity(candidates.len());
        for handle in handles {
            let batch = handle.join().map_err(|_| {
                ApiError::new(
                    "adapter.session.hash_worker_failed",
                    ErrorStage::Discovering,
                    "session discovery hash worker panicked",
                )
            })??;
            items.extend(batch);
        }
        Ok(items)
    })
}

pub(super) fn collect_session_manifest_items_parallel(
    plan: &SourcePlan,
    target: &SessionTarget,
    project_filter: Option<&str>,
    base_uri: &str,
    root: &Path,
) -> Result<Vec<ManifestItem>> {
    let mut builder = session_walk_builder(root);
    builder.threads(session_hash_threads(usize::MAX));
    let items = Mutex::new(Vec::new());
    let first_error = Mutex::new(None);

    builder.build_parallel().run(|| {
        Box::new(|entry| {
            if first_error
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .is_some()
            {
                return WalkState::Quit;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    record_session_parallel_error(&first_error, session_walk_error(err));
                    return WalkState::Quit;
                }
            };
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                return WalkState::Continue;
            }
            let path = entry.into_path();
            if !target_has_supported_session_extension(target, &path) {
                return WalkState::Continue;
            }
            let key = match relative_key(root, &path) {
                Ok(key) => key,
                Err(err) => {
                    record_session_parallel_error(&first_error, err);
                    return WalkState::Quit;
                }
            };
            if !matches_project_filter(project_filter, root, &path, &key) {
                return WalkState::Continue;
            }
            match session_manifest_item(plan, base_uri, root, &key) {
                Ok(Some(item)) => items
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push(item),
                Ok(None) => {}
                Err(err) => {
                    record_session_parallel_error(&first_error, err);
                    return WalkState::Quit;
                }
            }
            WalkState::Continue
        })
    });

    if let Some(error) = first_error
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner())
    {
        return Err(error);
    }
    let mut items = items
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

fn session_manifest_item(
    plan: &SourcePlan,
    base_uri: &str,
    root: &Path,
    key: &str,
) -> Result<Option<ManifestItem>> {
    let path = safe_item_path(root, key)?;
    let meta = fs::metadata(&path).map_err(|err| fs_error("stat_failed", &path, err))?;
    if !meta.is_file() {
        return Ok(None);
    }
    let content_hash = content_fingerprint(&path)?;
    let identity = item_identity(SourceKind::Session, base_uri, key)?;
    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("session_relative_path".to_string(), json!(key));
    Ok(Some(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::Transcript,
        content_kind: Some(ContentKind::Transcript),
        display_path: Some(key.to_string()),
        parent_key: None,
        size_bytes: Some(meta.len()),
        content_hash: Some(content_hash),
        mtime: modified_at(meta.modified().ok()),
        version: Some(SESSION_DOCUMENT_VERSION.to_string()),
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    }))
}

fn record_session_parallel_error(slot: &Mutex<Option<ApiError>>, error: ApiError) {
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn session_walk_error(err: ignore::Error) -> ApiError {
    ApiError::new(
        "adapter.session.walk_failed",
        ErrorStage::Discovering,
        err.to_string(),
    )
}

fn should_descend_entry(entry: &DirEntry) -> bool {
    entry.file_name().to_str() != Some(".git")
}

/// Supported session export extensions: `.jsonl` for Claude/Codex, `.json` for Gemini.
fn target_has_supported_session_extension(target: &SessionTarget, path: &Path) -> bool {
    matches!(
        (
            target.provider.as_str(),
            path.extension().and_then(|ext| ext.to_str())
        ),
        ("claude" | "codex", Some("jsonl")) | ("gemini", Some("json"))
    )
}

fn relative_key(root: &Path, file: &Path) -> Result<String> {
    if root.is_file() {
        let name = root.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            ApiError::new(
                "adapter.session.item_key.invalid",
                ErrorStage::Normalizing,
                "session item key must not be empty",
            )
        })?;
        return Ok(name.to_string());
    }
    let relative = file.strip_prefix(root).unwrap_or(file);
    let key = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if key.is_empty() {
        return Err(ApiError::new(
            "adapter.session.item_key.invalid",
            ErrorStage::Normalizing,
            "session item key must not be empty",
        ));
    }
    Ok(key)
}

pub(super) fn safe_item_path(root: &Path, key: &str) -> Result<PathBuf> {
    if root.is_file() {
        return Ok(root.to_path_buf());
    }
    if Path::new(key).is_absolute() || key.split('/').any(|part| part == "..") {
        return Err(ApiError::new(
            "adapter.session.path.escape",
            ErrorStage::Fetching,
            "session item key must stay inside the sessions root",
        )
        .with_context("key", key.to_string()));
    }
    Ok(root.join(key))
}

fn content_fingerprint(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|err| fs_error("read_failed", path, err))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| fs_error("read_failed", path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_prefix(&hasher.finalize(), 16))
}
