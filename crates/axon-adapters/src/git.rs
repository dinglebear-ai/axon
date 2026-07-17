//! Git repository source adapter (GitHub / GitLab / Gitea / generic git).
//!
//! The adapter owns repository materialization: [`GitSourceAdapter::materialize`]
//! validates and shallow-clones the routed target, stamps the checkout path on
//! the plan, and retains the temporary checkout through the service bridge.

mod acquire;
mod metadata;
mod target;
mod vertical;

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

pub use self::acquire::{clone_git_repo, is_git_target};
use self::metadata::git_source_document;
pub use self::target::{GitTarget, parse_git_target};

pub const MODULE_NAME: &str = "git";

const ADAPTER_NAME: &str = "git";

#[derive(Debug, Clone, Default)]
pub struct GitSourceAdapter;

impl GitSourceAdapter {
    pub fn new() -> Self {
        Self
    }

    pub async fn materialize(
        &self,
        mut plan: SourcePlan,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_adapter(&plan)?;
        // GitHub sub-page scopes (issue/PR/release) resolve a single API
        // document through a vertical extractor; there is no repository to
        // clone, so materialization is a no-op that keeps the plan flowing into
        // the shared pipeline without a checkout.
        if vertical::is_vertical(&plan) {
            return Ok(crate::acquisition::MaterializedSource::virtual_source(plan));
        }
        let checkout = clone_git_repo(&plan.request.source).await.map_err(|err| {
            crate::acquisition::materialization_error("adapter.git.clone_failed", err.to_string())
        })?;
        plan.route.validated_options.values.insert(
            "repo_root".to_string(),
            json!(checkout.path().to_string_lossy()),
        );
        Ok(crate::acquisition::MaterializedSource::temporary(
            plan, checkout,
        ))
    }
}

#[async_trait]
impl SourceAdapter for GitSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(git_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        if vertical::is_vertical(plan) {
            return vertical::discover(plan);
        }
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
        if vertical::is_vertical(plan) {
            return vertical::acquire(plan, diff).await;
        }
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
        validate_adapter(plan)?;
        if vertical::is_vertical(plan) {
            return vertical::normalize(plan, acquisition);
        }
        let target = git_target(plan)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| git_source_document(plan, &target, &acquisition, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "git_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn git_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Git,
        SourceScope::Repo,
    )
    .with_scope(SourceScope::Directory)
    // GitHub sub-page scopes are served by vertical extraction (see
    // `git::vertical`), not a clone. `axon-route`'s `github` adapter declares
    // the same scopes, so routing already selects this adapter for them.
    .with_scope(SourceScope::Issue)
    .with_scope(SourceScope::PullRequest)
    .with_scope(SourceScope::Release)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    git_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = git_target(plan)?;
    let root = repo_root(plan)?;

    let mut files = collect_files(&root)?;
    files.sort();

    let base_uri = target.web_url.trim_end_matches('/').to_string();
    let mut items = Vec::new();
    for file in files {
        let key = relative_key(&root, &file)?;
        let path = safe_item_path(&root, &key)?;
        let meta = fs::metadata(&path).map_err(|err| fs_error("stat_failed", &path, err))?;
        if !meta.is_file() {
            continue;
        }
        let content_hash = content_fingerprint(&path)?;
        let identity = item_identity(SourceKind::Git, &base_uri, &key)?;
        let mut item_metadata = MetadataMap::new();
        item_metadata.insert("git_relative_path".to_string(), json!(key));
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: identity.canonical_uri,
            item_kind: ItemKind::RepoFile,
            content_kind: Some(content_kind_for(&file)),
            display_path: Some(key),
            parent_key: None,
            size_bytes: Some(meta.len()),
            content_hash: Some(content_hash),
            mtime: None,
            version: None,
            fetch_plan: None,
            metadata: item_metadata,
            graph_hints: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_git_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let root = repo_root(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let key = item
            .display_path
            .clone()
            .unwrap_or_else(|| item.source_item_key.0.clone());
        let path = safe_item_path(&root, &key)?;
        let text = fs::read_to_string(&path).map_err(|err| fs_error("read_failed", &path, err))?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText { text },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let target = git_target(plan)?;
    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "git_fetch",
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

fn manifest_metadata(target: &GitTarget) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("git_provider".to_string(), json!(target.provider));
    metadata.insert("git_host".to_string(), json!(target.host));
    metadata.insert("git_repo".to_string(), json!(target.repo));
    if let Some(owner) = &target.owner {
        metadata.insert("git_owner".to_string(), json!(owner));
    }
    metadata.insert("git_web_url".to_string(), json!(target.web_url));
    metadata
}

/// The prepared clone root, passed by the services bridge as a validated option.
fn repo_root(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("repo_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.git.repo_root.required",
                axon_error::ErrorStage::Planning,
                "git adapter requires a repo_root option pointing at a checked-out clone",
            )
        })
}

fn git_target(plan: &SourcePlan) -> Result<GitTarget> {
    parse_git_target(&plan.request.source)
}

/// `GitSourceAdapter` is the single implementation behind every git-family
/// adapter the router can select — `git`, `github`, `gitea`, `gitlab` — all of
/// which resolve to `SourceKind::Git`. Validate on the source *kind*, not the
/// exact adapter name: the resolver picks `github` for `github.com` URLs, and
/// keying off the literal name `"git"` rejected every real forge URL with
/// `adapter.git.mismatch` (seen live indexing a GitHub repo).
fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.source.source_kind == SourceKind::Git {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.git.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a non-git source kind",
    )
    .with_context("adapter", plan.route.adapter.name.clone())
    .with_context(
        "source_kind",
        format!("{:?}", plan.route.source.source_kind),
    ))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(false)
        .filter_entry(should_descend_entry);
    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|err| {
            ApiError::new(
                "adapter.git.walk_failed",
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
            axon_error::ErrorStage::Normalizing,
            "git item key must not be empty",
        ));
    }
    Ok(key)
}

fn safe_item_path(root: &Path, key: &str) -> Result<PathBuf> {
    if Path::new(key).is_absolute() || key.split('/').any(|part| part == "..") {
        return Err(ApiError::new(
            "adapter.git.path.escape",
            axon_error::ErrorStage::Fetching,
            "git item key must stay inside the repo root",
        )
        .with_context("key", key.to_string()));
    }
    Ok(root.join(key))
}

fn content_fingerprint(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|err| fs_error("read_failed", path, err))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
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

fn fs_error(code: &str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(
        format!("adapter.git.{code}"),
        axon_error::ErrorStage::Fetching,
        err.to_string(),
    )
    .with_context("path", path.display().to_string())
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.git.blocking_task_failed",
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
        stage_id: StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes())),
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

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

pub(crate) fn hex_prefix(digest: &[u8], hex_chars: usize) -> String {
    use std::fmt::Write as _;
    let mut token = String::with_capacity(hex_chars);
    for byte in &digest[..(hex_chars / 2).min(digest.len())] {
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
