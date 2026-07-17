//! Upload source adapter — staged content (single uploaded file, or an
//! already-unpacked archive/repomix bundle) materialized on disk by the
//! upload transport, converted into [`SourceDocument`]s.
//!
//! This mirrors the `local` adapter's filesystem walk (same select rules,
//! same `local_io` helpers) but carries a distinct `SourceKind::Upload`
//! identity so uploaded content is provenance-tracked separately from a
//! caller-specified local path, per the "Prepared Uploads" table in
//! `docs/pipeline-unification/sources/adapter-scopes.md`.
//!
//! The transport-facing upload store is injected through [`UploadSourceProvider`].
//! Materialization copies the authorized staged artifact into an adapter-owned
//! temporary directory; discovery never interprets an upload id as a
//! caller-controlled filesystem path.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::local::local_io::{content_fingerprint, fs_error, read_content_ref, safe_item_path};
use crate::local_select::{LocalOptions, is_binary_path, validate_options};
use crate::manifest::item_identity;

pub const MODULE_NAME: &str = "upload";

const ADAPTER_NAME: &str = "upload";

mod materialize;
pub use materialize::{UploadSourceProvider, upload_source_identity_from_uri};

#[derive(Debug, Clone, Default)]
pub struct UploadSourceAdapter;

impl UploadSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for UploadSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(upload_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || discover_sync(&plan))
            .await
            .map_err(blocking_join_error)?
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        let plan = plan.clone();
        let diff = diff.clone();
        tokio::task::spawn_blocking(move || acquire_sync(&plan, &diff))
            .await
            .map_err(blocking_join_error)?
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| upload_source_document(plan, &acquisition, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "upload_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn upload_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Upload,
        SourceScope::File,
    )
    .with_scope(SourceScope::Directory)
    .with_scope(SourceScope::Map)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    upload_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let options = validate_options(&plan.route.validated_options)?;

    let root = PathBuf::from(&plan.request.source);
    let files = match plan.route.scope {
        SourceScope::File => vec![root.clone()],
        SourceScope::Directory | SourceScope::Map => collect_files(&root, &options)?,
        _ => {
            return Err(ApiError::new(
                "adapter.upload.scope.unsupported",
                axon_error::ErrorStage::Routing,
                "upload adapter only discovers file/directory/map scopes",
            )
            .with_context("scope", format!("{:?}", plan.route.scope)));
        }
    };

    let base_uri = public_base_uri(&plan.route.source.canonical_uri);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let mut items = Vec::new();
    let mut sorted_files = files;
    sorted_files.sort();
    for file in sorted_files {
        let key = relative_key(root_for_keys, &file)?;
        if !options.should_include_file(plan.route.scope, &key, &file) {
            continue;
        }
        let metadata = fs::metadata(&file)
            .map_err(|err| fs_error("adapter.upload.stat_failed", &file, err))?;
        if let Some(max_file_bytes) = options.max_file_bytes
            && metadata.len() > max_file_bytes
        {
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let content_hash = content_fingerprint(&file)?;
        let identity = item_identity(SourceKind::Upload, &base_uri, &key)?;
        let mut item_metadata = MetadataMap::new();
        item_metadata.insert("staged_upload".to_string(), json!(true));
        item_metadata.insert(
            "upload_kind".to_string(),
            json!(upload_kind_for(plan.route.scope)),
        );
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: identity.canonical_uri,
            item_kind: ItemKind::LocalFile,
            content_kind: Some(content_kind_for(&file)),
            display_path: Some(key),
            parent_key: None,
            size_bytes: Some(metadata.len()),
            content_hash: Some(content_hash),
            mtime: modified_at(metadata.modified().ok()),
            version: None,
            fetch_plan: None,
            metadata: item_metadata,
            graph_hints: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_upload_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();

    let root = PathBuf::from(&plan.request.source);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let options = validate_options(&plan.route.validated_options)?;
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let path = safe_item_path(root_for_keys, &item.source_item_key.0)?;
        let content_ref = read_content_ref(&path, &options)?;
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
            "upload_fetch",
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

fn upload_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("upload"));
    metadata.insert("source_kind".to_string(), json!("upload"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert("staged_upload".to_string(), json!(true));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    SourceDocument {
        document_id: upload_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::PlainText),
        content: item.content_ref.clone(),
        metadata,
        title: item.manifest_item.display_path.clone(),
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn upload_kind_for(scope: SourceScope) -> &'static str {
    match scope {
        SourceScope::File => "file",
        SourceScope::Directory => "archive",
        SourceScope::Map => "map",
        _ => "unknown",
    }
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.upload.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn collect_files(root: &Path, options: &LocalOptions) -> Result<Vec<PathBuf>> {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(options.follow_symlinks)
        .hidden(false)
        .ignore(options.respect_gitignore)
        .git_ignore(options.respect_gitignore)
        .git_exclude(options.respect_gitignore)
        .git_global(options.respect_gitignore)
        .parents(options.respect_gitignore)
        .filter_entry(should_descend_entry);
    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|err| {
            ApiError::new(
                "adapter.upload.walk_failed",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
        })?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            files.push(entry.into_path());
        }
    }
    Ok(files)
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
            "adapter.upload.item_key.invalid",
            axon_error::ErrorStage::Normalizing,
            "upload item key must not be empty",
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
        && scheme == "upload"
    {
        return format!("upload://{}", rest.trim_matches('/'));
    }
    "upload://source".to_string()
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

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.upload.blocking_task_failed",
        axon_error::ErrorStage::Planning,
        err.to_string(),
    )
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

fn upload_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_upload_{}",
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

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
