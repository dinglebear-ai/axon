//! Git repository source adapter (GitHub / GitLab / Gitea / generic git).
//!
//! The adapter owns repository materialization: [`GitSourceAdapter::materialize`]
//! validates and shallow-clones the routed target, stamps the checkout path on
//! the plan, and retains the temporary checkout through the service bridge.

mod acquire;
mod discovery;
mod metadata;
mod target;
mod vertical;

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;

pub use self::acquire::{clone_git_repo, is_git_target};
use self::discovery::{
    collect_capped_git_keys, collect_git_manifest_items_parallel, hash_git_keys_parallel,
    safe_item_path,
};
use self::metadata::git_source_document;
pub use self::target::{GitTarget, parse_git_target};

pub const MODULE_NAME: &str = "git";

const ADAPTER_NAME: &str = "git";
const GIT_DISCOVERY_HASH_MAX_THREADS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct GitSourceAdapter;

impl GitSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for GitSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(git_capability(self.version()).into())
    }

    async fn materialize(
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
        let SourceAcquisition {
            source_id,
            fetched_items,
            ..
        } = acquisition;
        let documents = fetched_items
            .into_iter()
            .map(|item| git_source_document(plan, &target, &source_id, item))
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
    git_capability(crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION)
        .validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = git_target(plan)?;
    let root = repo_root(plan)?;

    let base_uri = target.web_url.trim_end_matches('/').to_string();
    let exclude_paths = option_string_array(&plan.request.options, "exclude_paths")?;
    let max_items = plan
        .limits
        .effective
        .max_items
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX));
    let mut items = if let Some(limit) = max_items {
        let keys = collect_capped_git_keys(&root, &exclude_paths, limit)?;
        hash_git_keys_parallel(plan, &root, &base_uri, &keys)?
    } else {
        collect_git_manifest_items_parallel(plan, &root, &base_uri, &exclude_paths)?
    };
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

fn option_string_array(options: &AdapterOptions, key: &str) -> Result<Vec<String>> {
    let Some(value) = options.values.get(key) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(ApiError::new(
            "adapter.git.option.invalid",
            ErrorStage::Routing,
            format!("git adapter option `{key}` must be an array of strings"),
        ));
    };
    values
        .iter()
        .map(|value| {
            value.as_str().map(ToString::to_string).ok_or_else(|| {
                ApiError::new(
                    "adapter.git.option.invalid",
                    ErrorStage::Routing,
                    format!("git adapter option `{key}` entries must be strings"),
                )
            })
        })
        .collect()
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
                ErrorStage::Planning,
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
        ErrorStage::Routing,
        "route selected a non-git source kind",
    )
    .with_context("adapter", plan.route.adapter.name.clone())
    .with_context(
        "source_kind",
        format!("{:?}", plan.route.source.source_kind),
    ))
}

fn fs_error(code: &str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(
        format!("adapter.git.{code}"),
        ErrorStage::Fetching,
        err.to_string(),
    )
    .with_context("path", path.display().to_string())
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.git.blocking_task_failed",
        ErrorStage::Planning,
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
