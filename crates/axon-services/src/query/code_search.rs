use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
    OperationKind, RedactionMetadata, SourceGenerationId, SourceId, SourceRange,
};
use axon_api::{CanonicalCitation, QueryHit};
use axon_core::config::Config;

mod result;
use result::*;

use super::provider_execution::ReadExecution;
use crate::context::ServiceContext;
use crate::query::wrap_service_error;
use crate::types::{CodeSearchCaller, CodeSearchFreshness, CodeSearchOptions, CodeSearchResult};

pub use self::refresh::{
    CodeSearchProjectResult, CodeSearchRefreshResult, refresh_code_search_index,
    refresh_code_search_index_with_progress, resolve_code_search_project,
};
use self::request::target_code_search_request;
use self::support::{CodeIndexIdentity, CodeSearchAllowedRoots, validate_path_prefix};
pub use self::support::{FreshnessWarning, ReindexProgress, ReindexProgressSink};

#[path = "code_search_refresh.rs"]
mod refresh;
#[path = "code_search_request.rs"]
mod request;
#[path = "code_search_support.rs"]
mod support;

const MAX_CODE_SEARCH_QUERY_LEN_BYTES: usize = 64 * 1024;
const CODE_SEARCH_GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Search one local git checkout after optionally refreshing its local-code vectors.
#[must_use = "code_search returns a Result that should be handled"]
pub async fn code_search(
    ctx: &ServiceContext,
    text: &str,
    opts: CodeSearchOptions,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    code_search_with_progress(ctx.clone(), text.to_owned(), opts, None).await
}

/// Owned-input boundary for transport handlers whose futures must be `Send`.
pub async fn code_search_owned(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    code_search_with_progress(ctx, text, opts, None).await
}

/// Owned committed-index boundary for server projections, which never refresh.
pub async fn code_search_committed_owned(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    if text.len() > MAX_CODE_SEARCH_QUERY_LEN_BYTES {
        return Err(format!(
            "code_search query exceeds {MAX_CODE_SEARCH_QUERY_LEN_BYTES}-byte cap (got {} bytes)",
            text.len()
        )
        .into());
    }
    let path_prefix = opts
        .path_prefix
        .as_deref()
        .map(validate_path_prefix)
        .transpose()?
        .flatten();
    target_code_search_committed(ctx, text, opts, path_prefix).await
}

#[must_use = "code_search_with_progress returns a Result that should be handled"]
pub async fn code_search_with_progress(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
    progress: Option<std::sync::Arc<dyn ReindexProgressSink>>,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    if text.len() > MAX_CODE_SEARCH_QUERY_LEN_BYTES {
        return Err(format!(
            "code_search query exceeds {MAX_CODE_SEARCH_QUERY_LEN_BYTES}-byte cap (got {} bytes)",
            text.len()
        )
        .into());
    }

    let path_prefix = opts
        .path_prefix
        .as_deref()
        .map(validate_path_prefix)
        .transpose()?
        .flatten();
    target_code_search(ctx, text, opts, path_prefix, progress).await
}

async fn target_code_search(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
    path_prefix: Option<String>,
    progress: Option<std::sync::Arc<dyn ReindexProgressSink>>,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    let refresh = if opts.ensure_fresh {
        refresh::refresh_code_search_index_owned_with_progress(
            ctx.clone(),
            opts.cwd.clone(),
            opts.caller,
            progress,
        )
        .await?
    } else {
        let collection = opts.collection.as_deref().unwrap_or(&ctx.cfg().collection);
        refresh::target_code_search_committed_state_owned(
            ctx.clone(),
            opts.cwd.clone(),
            opts.caller,
            collection.to_owned(),
        )
        .await?
    };
    complete_target_code_search(ctx, text, opts, path_prefix, refresh).await
}

async fn target_code_search_committed(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
    path_prefix: Option<String>,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    let collection = opts
        .collection
        .clone()
        .unwrap_or_else(|| ctx.cfg().collection.clone());
    let refresh = refresh::target_code_search_committed_state_owned(
        ctx.clone(),
        opts.cwd.clone(),
        opts.caller,
        collection,
    )
    .await?;
    complete_target_code_search(ctx, text, opts, path_prefix, refresh).await
}

async fn complete_target_code_search(
    ctx: ServiceContext,
    text: String,
    opts: CodeSearchOptions,
    path_prefix: Option<String>,
    refresh: CodeSearchRefreshResult,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    let Some(source_id) = refresh.target_source_id.clone() else {
        return Ok(code_search_missing_index_result(&text, refresh.freshness));
    };
    let Some(generation) = refresh.target_source_generation.clone() else {
        return Ok(code_search_missing_index_result(&text, refresh.freshness));
    };
    if ctx.target_local_source_runtime().is_none() {
        return Ok(code_search_missing_index_result(&text, refresh.freshness));
    }
    let execution = ReadExecution::begin_owned(
        ctx.clone(),
        ctx.cfg().clone(),
        OperationKind::Query,
        serde_json::json!({
            "query": &text,
            "collection": ctx.cfg().collection,
            "source_id": source_id.0,
            "generation": generation.0,
            "operation": "code_search",
        }),
        None,
    )
    .await
    .map_err(|error| -> Box<dyn Error + Send + Sync> { error.to_string().into() })?;
    let provider = execution.scheduled_embedding();
    let store = execution.scheduled_vectors();
    let provider_id = execution.embedding_provider_id();
    let model = execution.embedding_model();

    let result: Result<CodeSearchResult, Box<dyn Error + Send + Sync>> = async {
        let embedding = provider
            .embed(EmbeddingBatch {
                batch_id: BatchId::new(uuid::Uuid::new_v4()),
                job_id: JobId::new(uuid::Uuid::new_v4()),
                provider_id,
                model,
                items: vec![EmbeddingInput {
                    chunk_id: ChunkId::new("code-search-query"),
                    text: text.clone(),
                    content_kind: ContentKind::Code,
                    metadata: MetadataMap::new(),
                }],
                instruction: None,
                priority: JobPriority::Interactive,
                metadata: MetadataMap::new(),
            })
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                wrap_service_error("target code_search query embedding failed".to_string(), &e)
            })?;
        let dense_vector = embedding
            .vectors
            .first()
            .map(|vector| vector.values.clone())
            .ok_or("target code_search query embedding returned no vector")?;

        let request = target_code_search_request(
            opts.collection
                .clone()
                .unwrap_or_else(|| ctx.cfg().collection.clone()),
            &text,
            opts.limit.saturating_add(opts.offset).max(1),
            dense_vector,
            &source_id,
            &generation,
            path_prefix.as_deref(),
            opts.language.as_deref(),
        )?;
        let matches = store
            .search(request)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                wrap_service_error("target code_search vector query failed".to_string(), &e)
            })?;
        let results = matches
            .results
            .into_iter()
            .skip(opts.offset)
            .take(opts.limit.clamp(1, axon_api::MAX_CANONICAL_CITATIONS))
            .enumerate()
            .map(|(index, hit)| target_vector_match_to_query_hit(index as u64 + 1, hit))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CodeSearchResult {
            query: text.clone(),
            content_trust: "untrusted_local_code".to_string(),
            results,
            freshness: refresh.freshness,
        })
    }
    .await;
    execution.finish_owned(ctx, &result).await;
    result
}

fn code_search_missing_index_result(
    text: &str,
    freshness: CodeSearchFreshness,
) -> CodeSearchResult {
    CodeSearchResult {
        query: text.to_string(),
        content_trust: "untrusted_local_code".to_string(),
        results: Vec::new(),
        freshness: code_search_missing_index_freshness(freshness),
    }
}

pub(crate) fn code_search_freshness(
    status: &str,
    warning: Option<FreshnessWarning>,
    indexed_files: usize,
    removed_files: usize,
) -> CodeSearchFreshness {
    let status = if warning.is_some() { "stale" } else { status };
    CodeSearchFreshness {
        status: status.to_string(),
        warning: warning.map(|warning| warning.message()),
        indexed_files,
        removed_files,
    }
}

pub(crate) fn code_search_missing_index_freshness(
    mut freshness: CodeSearchFreshness,
) -> CodeSearchFreshness {
    if freshness.warning.is_none() {
        freshness.status = "stale".to_string();
        freshness.warning = Some(FreshnessWarning::MissingCommittedIndex.message());
    }
    freshness
}

pub(crate) async fn resolve_code_search_root(
    cwd: Option<PathBuf>,
    caller: CodeSearchCaller,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let cwd = match (caller, cwd) {
        (CodeSearchCaller::Cli, Some(cwd)) => cwd,
        (CodeSearchCaller::Cli, None) => std::env::current_dir()?,
        (CodeSearchCaller::Mcp, Some(cwd)) => cwd,
        (CodeSearchCaller::Mcp, None) => {
            return Err("code_search MCP requests must provide cwd".into());
        }
        (CodeSearchCaller::Rest, Some(cwd)) => cwd,
        (CodeSearchCaller::Rest, None) => {
            return Err("code_search REST requests must provide an explicit root".into());
        }
    };
    let canonical_cwd = tokio::fs::canonicalize(&cwd)
        .await
        .map_err(|_| "code_search cwd could not be resolved")?;
    let git_root = git_toplevel(&canonical_cwd).await?;
    reject_unsafe_code_root(&git_root)?;
    if matches!(caller, CodeSearchCaller::Mcp | CodeSearchCaller::Rest) {
        let allowed = CodeSearchAllowedRoots::from_env().await?;
        if !allowed.contains(&git_root) {
            return Err(code_search_outside_allowed_roots_message().into());
        }
    }
    Ok(git_root)
}

pub(crate) fn code_search_outside_allowed_roots_message() -> &'static str {
    "code_search cwd is outside AXON_CODE_SEARCH_ALLOWED_ROOTS"
}

async fn git_toplevel(cwd: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let cwd = cwd.to_path_buf();
    let output = tokio::time::timeout(
        CODE_SEARCH_GIT_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(["rev-parse", "--show-toplevel"])
                .output()
        }),
    )
    .await
    .map_err(|_| "git rev-parse timed out")?
    .map_err(|e| format!("git rev-parse task failed: {e}"))?
    .map_err(|_| "code_search cwd is not inside a git checkout")?;
    if !output.status.success() {
        return Err("code_search cwd is not inside a git checkout".into());
    }
    let root = String::from_utf8(output.stdout)
        .map_err(|e| format!("git rev-parse output was not UTF-8: {e}"))?;
    let root = root.trim();
    if root.is_empty() {
        return Err("git rev-parse returned an empty repository root".into());
    }
    tokio::fs::canonicalize(root).await.map_err(Into::into)
}

fn reject_unsafe_code_root(root: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    if root == Path::new("/") {
        return Err("code_search refuses to index filesystem root".into());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && root == home.as_path()
    {
        return Err("code_search refuses to index HOME directly".into());
    }
    Ok(())
}

pub(super) async fn code_search_identity(
    cfg: Config,
    project_root: PathBuf,
    collection: String,
) -> CodeIndexIdentity {
    let origin = code_search_project_origin(&project_root).await;
    let embedder = if cfg.tei_url.trim().is_empty() {
        "tei".to_string()
    } else {
        cfg.tei_url.clone()
    };
    CodeIndexIdentity::new(project_root, origin, &collection, &embedder)
}

pub(crate) async fn code_search_project_origin(project_root: &Path) -> String {
    let remote = match git_remote_origin(project_root).await {
        Ok(Some(remote)) => remote,
        Ok(None) => "git:no-origin".to_string(),
        Err(error) => {
            tracing::warn!(
                %error,
                project_root = %project_root.display(),
                "code_search git remote origin lookup failed; using checkout-scoped fallback"
            );
            "git:no-origin".to_string()
        }
    };
    // This seed is private input to the UUID project key. Only the derived key is
    // stored in Qdrant payloads; the absolute root remains SQLite-only.
    format!("{remote}\nworktree:{}", project_root.display())
}

async fn git_remote_origin(project_root: &Path) -> Result<Option<String>, String> {
    let project_root = project_root.to_path_buf();
    let output = tokio::time::timeout(
        CODE_SEARCH_GIT_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["config", "--get", "remote.origin.url"])
                .output()
        }),
    )
    .await
    .map_err(|_| "git remote origin lookup timed out".to_string())?
    .map_err(|err| format!("git remote origin lookup task failed: {err}"))?
    .map_err(|err| format!("git remote origin lookup failed to spawn git: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let origin = String::from_utf8(output.stdout)
        .map_err(|err| format!("git remote origin output was not UTF-8: {err}"))?;
    let origin = origin.trim();
    Ok((!origin.is_empty()).then(|| format!("git:{origin}")))
}

#[cfg(test)]
#[path = "code_search_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "code_search_target_tests.rs"]
mod target_tests;
