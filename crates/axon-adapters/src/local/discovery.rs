//! Deterministic local-file discovery and manifest fingerprinting.

use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::adapter::Result;
use crate::local_select::{LocalOptions, is_binary_path};
use crate::manifest::item_identity;

use super::LOCAL_DISCOVERY_HASH_MAX_THREADS;
use super::local_io::{LocalRootHandle, content_fingerprint_from_file, fs_error};
use super::modified_at;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct LocalFileCandidate {
    key: String,
    path: PathBuf,
}

pub(super) fn collect_capped_file_candidates(
    root: &Path,
    root_for_keys: &Path,
    scope: SourceScope,
    options: &LocalOptions,
    root_handle: &LocalRootHandle,
    limit: usize,
) -> Result<Vec<LocalFileCandidate>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    // The executor historically sorted the full manifest and truncated it after
    // discovery. Keep those exact deterministic first-N semantics without
    // retaining or hashing every file: a max-heap retains only the N smallest
    // source item keys seen during the walk.
    let mut selected = BinaryHeap::with_capacity(limit.min(4096));
    visit_local_files(root, options, |path| {
        let Some(candidate) =
            local_file_candidate(root_for_keys, scope, options, root_handle, path)?
        else {
            return Ok(());
        };
        if selected.len() < limit {
            selected.push(candidate);
        } else if selected.peek().is_some_and(|largest| candidate < *largest) {
            selected.pop();
            selected.push(candidate);
        }
        Ok(())
    })?;

    let mut selected = selected.into_vec();
    selected.sort();
    Ok(selected)
}

fn local_walk_builder(root: &Path, options: &LocalOptions) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(options.follow_symlinks)
        .hidden(false)
        .ignore(options.respect_gitignore)
        .git_ignore(options.respect_gitignore)
        .git_exclude(options.respect_gitignore)
        .git_global(options.respect_gitignore)
        .parents(options.respect_gitignore);
    if options.should_prune_default_dirs() {
        builder.filter_entry(should_descend_entry);
    }
    builder
}

fn visit_local_files(
    root: &Path,
    options: &LocalOptions,
    mut visitor: impl FnMut(PathBuf) -> Result<()>,
) -> Result<()> {
    for entry in local_walk_builder(root, options).build() {
        let entry = entry.map_err(local_walk_error)?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            visitor(entry.into_path())?;
        }
    }
    Ok(())
}

fn local_hash_threads(work_items: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    cpus.div_ceil(2)
        .clamp(1, LOCAL_DISCOVERY_HASH_MAX_THREADS)
        .min(work_items.max(1))
}

pub(super) fn hash_file_candidates_parallel(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    options: &LocalOptions,
    base_uri: &str,
    candidates: &[LocalFileCandidate],
) -> Result<Vec<ManifestItem>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let threads = local_hash_threads(candidates.len());
    if threads == 1 {
        let mut items = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(item) =
                manifest_item_from_candidate(plan, root_handle, options, base_uri, candidate)?
            {
                items.push(item);
            }
        }
        return Ok(items);
    }

    let chunk_size = candidates.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for chunk in candidates.chunks(chunk_size) {
            handles.push(scope.spawn(move || -> Result<Vec<ManifestItem>> {
                let mut items = Vec::with_capacity(chunk.len());
                for candidate in chunk {
                    if let Some(item) = manifest_item_from_candidate(
                        plan,
                        root_handle,
                        options,
                        base_uri,
                        candidate,
                    )? {
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
                    "adapter.local.hash_worker_failed",
                    ErrorStage::Discovering,
                    "local discovery hash worker panicked",
                )
            })??;
            items.extend(batch);
        }
        Ok(items)
    })
}

pub(super) fn collect_manifest_items_parallel(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    options: &LocalOptions,
    base_uri: &str,
    root_for_keys: &Path,
    root: &Path,
) -> Result<Vec<ManifestItem>> {
    let mut builder = local_walk_builder(root, options);
    builder.threads(local_hash_threads(usize::MAX));
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
                    record_parallel_walk_error(&first_error, local_walk_error(err));
                    return WalkState::Quit;
                }
            };
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                return WalkState::Continue;
            }
            match manifest_item_from_path(
                plan,
                root_handle,
                options,
                base_uri,
                root_for_keys,
                entry.into_path(),
            ) {
                Ok(Some(item)) => items
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push(item),
                Ok(None) => {}
                Err(err) => {
                    record_parallel_walk_error(&first_error, err);
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

fn record_parallel_walk_error(slot: &Mutex<Option<ApiError>>, error: ApiError) {
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn local_walk_error(err: ignore::Error) -> ApiError {
    ApiError::new(
        "adapter.local.walk_failed",
        ErrorStage::Discovering,
        err.to_string(),
    )
}

fn local_file_candidate(
    root_for_keys: &Path,
    scope: SourceScope,
    options: &LocalOptions,
    root_handle: &LocalRootHandle,
    path: PathBuf,
) -> Result<Option<LocalFileCandidate>> {
    let key = relative_key(root_for_keys, &path)?;
    if !options.should_include_file(scope, &key, &path) {
        return Ok(None);
    }
    let file = root_handle.open_file(&key)?;
    let metadata = file
        .metadata()
        .map_err(|err| fs_error("adapter.local.stat_failed", &path, err))?;
    if !metadata.is_file()
        || options
            .max_file_bytes
            .is_some_and(|max_bytes| metadata.len() > max_bytes)
    {
        return Ok(None);
    }
    Ok(Some(LocalFileCandidate { key, path }))
}

pub(super) fn manifest_item_from_path(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    options: &LocalOptions,
    base_uri: &str,
    root_for_keys: &Path,
    path: PathBuf,
) -> Result<Option<ManifestItem>> {
    let key = relative_key(root_for_keys, &path)?;
    if !options.should_include_file(plan.route.scope, &key, &path) {
        return Ok(None);
    }
    manifest_item_from_open_path(plan, root_handle, options, base_uri, &key, &path)
}

fn manifest_item_from_candidate(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    options: &LocalOptions,
    base_uri: &str,
    candidate: &LocalFileCandidate,
) -> Result<Option<ManifestItem>> {
    manifest_item_from_open_path(
        plan,
        root_handle,
        options,
        base_uri,
        &candidate.key,
        &candidate.path,
    )
}

fn manifest_item_from_open_path(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    options: &LocalOptions,
    base_uri: &str,
    key: &str,
    path: &Path,
) -> Result<Option<ManifestItem>> {
    let file = root_handle.open_file(key)?;
    let metadata = file
        .metadata()
        .map_err(|err| fs_error("adapter.local.stat_failed", path, err))?;
    if !metadata.is_file()
        || options
            .max_file_bytes
            .is_some_and(|max_bytes| metadata.len() > max_bytes)
    {
        return Ok(None);
    }
    let content_hash = content_fingerprint_from_file(file, path)?;
    let identity = item_identity(SourceKind::Local, base_uri, key)?;
    Ok(Some(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::LocalFile,
        content_kind: Some(content_kind_for(path)),
        display_path: Some(key.to_string()),
        parent_key: None,
        size_bytes: Some(metadata.len()),
        content_hash: Some(content_hash),
        mtime: modified_at(metadata.modified().ok()),
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }))
}

fn should_descend_entry(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !crate::local_select::is_pruned_dir(name)
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
            "adapter.local.item_key.invalid",
            ErrorStage::Normalizing,
            "local item key must not be empty",
        ));
    }
    Ok(key)
}

pub(super) fn root_for_item_keys(root: &Path, scope: SourceScope) -> &Path {
    if scope == SourceScope::File {
        return root.parent().unwrap_or_else(|| Path::new(""));
    }
    if root.is_file() {
        root.parent().unwrap_or_else(|| Path::new(""))
    } else {
        root
    }
}

pub(super) fn public_base_uri(canonical_uri: &str) -> String {
    if let Some((scheme, rest)) = canonical_uri.split_once("://")
        && scheme == "local"
    {
        return format!("local://{}", rest.trim_matches('/'));
    }
    "local://source".to_string()
}

fn content_kind_for(path: &Path) -> ContentKind {
    if is_binary_path(path) {
        return ContentKind::BinaryMetadata;
    }
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
