//! Git checkout discovery, bounded hashing, and manifest construction.

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

use super::{GIT_DISCOVERY_HASH_MAX_THREADS, fs_error, hex_prefix};

fn git_walk_builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(false)
        .filter_entry(should_descend_entry);
    builder
}

fn git_hash_threads(work_items: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    cpus.div_ceil(2)
        .clamp(1, GIT_DISCOVERY_HASH_MAX_THREADS)
        .min(work_items.max(1))
}

pub(super) fn collect_capped_git_keys(
    root: &Path,
    exclude_paths: &[String],
    limit: usize,
) -> Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut selected = BinaryHeap::with_capacity(limit.min(4096));
    for entry in git_walk_builder(root).build() {
        let entry = entry.map_err(git_walk_error)?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let key = relative_key(root, entry.path())?;
        if git_key_excluded(&key, exclude_paths) {
            continue;
        }
        if selected.len() < limit {
            selected.push(key);
        } else if selected.peek().is_some_and(|largest| key < *largest) {
            selected.pop();
            selected.push(key);
        }
    }
    let mut selected = selected.into_vec();
    selected.sort();
    Ok(selected)
}

pub(super) fn hash_git_keys_parallel(
    plan: &SourcePlan,
    root: &Path,
    base_uri: &str,
    keys: &[String],
) -> Result<Vec<ManifestItem>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let threads = git_hash_threads(keys.len());
    if threads == 1 {
        let mut items = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(item) = git_manifest_item(plan, root, base_uri, key)? {
                items.push(item);
            }
        }
        return Ok(items);
    }

    let chunk_size = keys.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for chunk in keys.chunks(chunk_size) {
            handles.push(scope.spawn(move || -> Result<Vec<ManifestItem>> {
                let mut items = Vec::with_capacity(chunk.len());
                for key in chunk {
                    if let Some(item) = git_manifest_item(plan, root, base_uri, key)? {
                        items.push(item);
                    }
                }
                Ok(items)
            }));
        }
        let mut items = Vec::with_capacity(keys.len());
        for handle in handles {
            let batch = handle.join().map_err(|_| {
                ApiError::new(
                    "adapter.git.hash_worker_failed",
                    ErrorStage::Discovering,
                    "git discovery hash worker panicked",
                )
            })??;
            items.extend(batch);
        }
        Ok(items)
    })
}

pub(super) fn collect_git_manifest_items_parallel(
    plan: &SourcePlan,
    root: &Path,
    base_uri: &str,
    exclude_paths: &[String],
) -> Result<Vec<ManifestItem>> {
    let mut builder = git_walk_builder(root);
    builder.threads(git_hash_threads(usize::MAX));
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
                    record_git_parallel_error(&first_error, git_walk_error(err));
                    return WalkState::Quit;
                }
            };
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                return WalkState::Continue;
            }
            let key = match relative_key(root, entry.path()) {
                Ok(key) => key,
                Err(err) => {
                    record_git_parallel_error(&first_error, err);
                    return WalkState::Quit;
                }
            };
            if git_key_excluded(&key, exclude_paths) {
                return WalkState::Continue;
            }
            match git_manifest_item(plan, root, base_uri, &key) {
                Ok(Some(item)) => items
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push(item),
                Ok(None) => {}
                Err(err) => {
                    record_git_parallel_error(&first_error, err);
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

fn git_manifest_item(
    plan: &SourcePlan,
    root: &Path,
    base_uri: &str,
    key: &str,
) -> Result<Option<ManifestItem>> {
    let path = safe_item_path(root, key)?;
    let meta = fs::metadata(&path).map_err(|err| fs_error("stat_failed", &path, err))?;
    if !meta.is_file() {
        return Ok(None);
    }
    let content_hash = content_fingerprint(&path)?;
    let identity = item_identity(SourceKind::Git, base_uri, key)?;
    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("git_relative_path".to_string(), json!(key));
    Ok(Some(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::RepoFile,
        content_kind: Some(content_kind_for(&path)),
        display_path: Some(key.to_string()),
        parent_key: None,
        size_bytes: Some(meta.len()),
        content_hash: Some(content_hash),
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    }))
}

fn git_key_excluded(key: &str, exclude_paths: &[String]) -> bool {
    exclude_paths.iter().any(|excluded| key.contains(excluded))
}

fn record_git_parallel_error(slot: &Mutex<Option<ApiError>>, error: ApiError) {
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn git_walk_error(err: ignore::Error) -> ApiError {
    ApiError::new(
        "adapter.git.walk_failed",
        ErrorStage::Discovering,
        err.to_string(),
    )
}

fn should_descend_entry(entry: &DirEntry) -> bool {
    entry.file_name().to_str() != Some(".git")
}

fn relative_key(root: &Path, file: &Path) -> Result<String> {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let key = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if key.is_empty() {
        return Err(ApiError::new(
            "adapter.git.item_key.invalid",
            ErrorStage::Normalizing,
            "git item key must not be empty",
        ));
    }
    Ok(key)
}

pub(super) fn safe_item_path(root: &Path, key: &str) -> Result<PathBuf> {
    if Path::new(key).is_absolute() || key.split('/').any(|part| part == "..") {
        return Err(ApiError::new(
            "adapter.git.path.escape",
            ErrorStage::Fetching,
            "git item key must stay inside the repo root",
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

fn content_kind_for(path: &Path) -> ContentKind {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "md" | "markdown" => ContentKind::Markdown,
        "html" | "htm" => ContentKind::Html,
        "json" => ContentKind::Json,
        "yaml" | "yml" => ContentKind::Yaml,
        "toml" => ContentKind::Toml,
        "xml" => ContentKind::Xml,
        "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "py" | "java" | "kt" | "swift" | "c" | "cc"
        | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "sh" | "zsh" | "fish" => ContentKind::Code,
        _ => ContentKind::PlainText,
    }
}
