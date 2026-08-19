//! Local filesystem source adapter.

pub(crate) mod local_io;
mod root_state;

use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_trait::async_trait;
use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder, WalkState};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::local_select::{LocalOptions, is_binary_path, validate_options};
use crate::manifest::item_identity;

use self::local_io::{
    LocalRootHandle, content_fingerprint_from_file, fs_error, read_content_ref_from_file,
};
pub use self::root_state::LocalSourceAdapter;

pub const MODULE_NAME: &str = "local";

const ADAPTER_NAME: &str = "local";
const LOCAL_DISCOVERY_HASH_MAX_THREADS: usize = 8;
#[async_trait]
impl SourceAdapter for LocalSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(local_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        let root_handle = self.root_for_discovery(plan)?;
        let retained_handle = Arc::clone(&root_handle);
        let job_id = plan.job_id;
        let plan = plan.clone();
        let manifest = tokio::task::spawn_blocking(move || discover_sync(&plan, &root_handle))
            .await
            .map_err(blocking_join_error)??;
        self.retain_discovered_root(job_id, retained_handle)?;
        Ok(manifest)
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        let root_handle = self.held_root_for_acquisition(plan)?;
        let plan = plan.clone();
        let diff = diff.clone();
        tokio::task::spawn_blocking(move || acquire_sync(&plan, &diff, &root_handle))
            .await
            .map_err(blocking_join_error)?
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        let SourceAcquisition {
            source_id,
            fetched_items,
            ..
        } = acquisition;
        let documents = fetched_items
            .into_iter()
            .map(|item| local_source_document(plan, &source_id, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "local_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }

    fn release(&self, plan: &SourcePlan) {
        self.release_root(plan.job_id);
    }
}

fn local_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Local,
        SourceScope::File,
    )
    .with_scope(SourceScope::Directory)
    .with_scope(SourceScope::Workspace)
    .with_scope(SourceScope::Repo)
    .with_scope(SourceScope::Map)
}

fn discover_sync(plan: &SourcePlan, root_handle: &LocalRootHandle) -> Result<SourceManifest> {
    let capability = local_capability(crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION);
    capability.validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let options = validate_options(&plan.route.validated_options)?;
    if options.follow_symlinks {
        return Err(ApiError::new(
            "adapter.local.symlinks_unsupported",
            ErrorStage::Authorizing,
            "contained local sources do not follow symlinks",
        ));
    }

    let root = PathBuf::from(&plan.request.source);
    let base_uri = public_base_uri(&plan.route.source.canonical_uri);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let max_items = plan
        .limits
        .effective
        .max_items
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX));

    let mut items = match plan.route.scope {
        SourceScope::File => {
            if max_items == Some(0) {
                Vec::new()
            } else {
                manifest_item_from_path(
                    plan,
                    root_handle,
                    &options,
                    &base_uri,
                    root_for_keys,
                    root.clone(),
                )?
                .into_iter()
                .collect()
            }
        }
        SourceScope::Directory | SourceScope::Workspace | SourceScope::Repo | SourceScope::Map => {
            if let Some(limit) = max_items {
                let candidates = collect_capped_file_candidates(
                    &root,
                    root_for_keys,
                    plan.route.scope,
                    &options,
                    root_handle,
                    limit,
                )?;
                hash_file_candidates_parallel(plan, root_handle, &options, &base_uri, &candidates)?
            } else {
                collect_manifest_items_parallel(
                    plan,
                    root_handle,
                    &options,
                    &base_uri,
                    root_for_keys,
                    &root,
                )?
            }
        }
        _ => {
            return Err(ApiError::new(
                "adapter.local.scope.unsupported",
                ErrorStage::Routing,
                "local adapter only discovers file-like local scopes",
            )
            .with_context("scope", format!("{:?}", plan.route.scope)));
        }
    };
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_local_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    })
}

fn acquire_sync(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
    root_handle: &LocalRootHandle,
) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    if plan.route.scope == SourceScope::Map {
        return Ok(SourceAcquisition {
            header: stage_header(plan.job_id, "local_fetch", PipelinePhase::Fetching, 0),
            source_id: plan.route.source.source_id.clone(),
            generation: diff.next_generation.clone(),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            manifest: SourceManifest {
                source_id: plan.route.source.source_id.clone(),
                generation: diff.next_generation.clone(),
                adapter: plan.route.adapter.clone(),
                scope: plan.route.scope,
                items: diff
                    .added
                    .iter()
                    .chain(diff.modified.iter())
                    .cloned()
                    .collect(),
                created_at: timestamp(),
                metadata: MetadataMap::new(),
            },
            fetched_items: Vec::new(),
            artifacts: Vec::new(),
        });
    }
    let root = PathBuf::from(&plan.request.source);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let options = validate_options(&plan.route.validated_options)?;
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let path = root_for_keys.join(&item.source_item_key.0);
        if !options.fetches_body(&path) {
            continue;
        }
        let content_ref = read_content_ref_from_file(
            root_handle.open_file(&item.source_item_key.0)?,
            &path,
            &options,
        )?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref,
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    };

    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "local_fetch",
            PipelinePhase::Fetching,
            fetched_items.len(),
        ),
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        adapter: manifest.adapter.clone(),
        scope: manifest.scope,
        manifest,
        fetched_items,
        artifacts: Vec::new(),
    })
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.local.blocking_task_failed",
        ErrorStage::Planning,
        err.to_string(),
    )
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.local.mismatch",
        ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct LocalFileCandidate {
    key: String,
    path: PathBuf,
}

fn collect_capped_file_candidates(
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

fn hash_file_candidates_parallel(
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

fn collect_manifest_items_parallel(
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

fn manifest_item_from_path(
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

fn root_for_item_keys(root: &Path, scope: SourceScope) -> &Path {
    if scope == SourceScope::File {
        return root.parent().unwrap_or_else(|| Path::new(""));
    }
    if root.is_file() {
        root.parent().unwrap_or_else(|| Path::new(""))
    } else {
        root
    }
}

fn public_base_uri(canonical_uri: &str) -> String {
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

fn local_source_document(
    plan: &SourcePlan,
    source_id: &SourceId,
    item: AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("code"));
    metadata.insert("source_kind".to_string(), json!("local"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri.clone()),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    SourceDocument {
        document_id: local_document_id(source_id, &item.manifest_item.source_item_key),
        source_id: source_id.clone(),
        source_item_key: item.manifest_item.source_item_key,
        canonical_uri: item.manifest_item.canonical_uri,
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::PlainText),
        content: item.content_ref,
        metadata,
        title: item.manifest_item.display_path.clone(),
        language: None,
        path: item.manifest_item.display_path,
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id,
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn stage_header(
    job_id: JobId,
    stage_id: &'static str,
    phase: PipelinePhase,
    item_count: usize,
) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: named_stage_id(stage_id),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: Some(item_count as u64),
            items_done: item_count as u64,
            documents_total: Some(item_count as u64),
            documents_done: item_count as u64,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

fn named_stage_id(stage_id: &str) -> StageId {
    StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes()))
}

fn local_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_local_{}",
        stable_token(&format!("{}\0{}", source_id.0, item_key.0))
    ))
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn modified_at(modified: Option<SystemTime>) -> Option<Timestamp> {
    modified.map(|time| Timestamp(chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()))
}
