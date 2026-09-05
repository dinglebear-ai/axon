use std::error::Error;
use std::path::{Path, PathBuf};

use axon_api::source::{LifecycleStatus, SourceGenerationId, SourceId, SourceRequest, SourceScope};

use crate::context::ServiceContext;
use crate::source::index_source;
use crate::source::local_identity::local_source_id;
use crate::types::{CodeSearchCaller, CodeSearchFreshness};

use super::{
    CodeIndexIdentity, FreshnessWarning, ReindexProgress, ReindexProgressSink,
    code_search_freshness, code_search_identity, resolve_code_search_root,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CodeSearchRefreshResult {
    pub project_root: PathBuf,
    pub project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_source_generation: Option<SourceGenerationId>,
    pub freshness: CodeSearchFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CodeSearchProjectResult {
    pub project_root: PathBuf,
    pub project_key: String,
}

#[must_use = "resolve_code_search_project returns a Result that should be handled"]
pub async fn resolve_code_search_project(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
) -> Result<CodeSearchProjectResult, Box<dyn Error + Send + Sync>> {
    let root = resolve_code_search_root(cwd, caller).await?;
    let identity =
        code_search_identity(ctx.cfg().clone(), root, ctx.cfg().collection.clone()).await;
    Ok(CodeSearchProjectResult {
        project_root: identity.project_root,
        project_key: identity.project_key,
    })
}

#[must_use = "refresh_code_search_index returns a Result that should be handled"]
pub async fn refresh_code_search_index(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    refresh_code_search_index_with_progress(ctx, cwd, caller, None).await
}

#[must_use = "refresh_code_search_index_with_progress returns a Result that should be handled"]
pub async fn refresh_code_search_index_with_progress(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
    progress: Option<std::sync::Arc<dyn ReindexProgressSink>>,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    refresh_code_search_index_owned_with_progress(
        ctx.clone(),
        cwd.map(Path::to_path_buf),
        caller,
        progress,
    )
    .await
}

pub(super) async fn refresh_code_search_index_owned_with_progress(
    ctx: ServiceContext,
    cwd: Option<PathBuf>,
    caller: CodeSearchCaller,
    progress: Option<std::sync::Arc<dyn ReindexProgressSink>>,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    let root = resolve_code_search_root(cwd.as_deref(), caller).await?;
    let identity =
        code_search_identity(ctx.cfg().clone(), root, ctx.cfg().collection.clone()).await;
    let Some(_) = ctx.target_local_source_runtime() else {
        return Ok(target_refresh_unavailable_result(identity));
    };
    let project_root = identity.project_root.clone();
    let project_key = identity.project_key.clone();
    let mut request = SourceRequest::local_path(project_root.to_string_lossy(), true);
    request.scope = Some(SourceScope::Repo);
    request.collection = Some(ctx.cfg().collection.clone());
    emit_target_progress_started(progress.as_deref());
    match index_source(request, &ctx).await {
        Ok(output) if output.status == LifecycleStatus::Completed => {
            emit_target_progress_finished(progress.as_deref());
            let generation = output
                .ledger
                .committed_generation
                .clone()
                .unwrap_or(output.ledger.generation);
            let result = CodeSearchRefreshResult {
                project_root: project_root.clone(),
                project_key: project_key.clone(),
                target_source_id: Some(output.source_id),
                target_source_generation: Some(generation),
                freshness: code_search_freshness(
                    "fresh",
                    None,
                    usize::try_from(output.counts.documents_total).unwrap_or(usize::MAX),
                    0,
                ),
            };
            tracing::debug!(
                project_key,
                indexed_files = result.freshness.indexed_files,
                "target local source refresh completed for code-search"
            );
            Ok(result)
        }
        Ok(output) => {
            let warning = output
                .errors
                .first()
                .and_then(|error| error.cause.clone().or_else(|| Some(error.message.clone())))
                .or_else(|| {
                    output
                        .warnings
                        .first()
                        .map(|warning| warning.message.clone())
                })
                .unwrap_or_else(|| format!("source refresh ended as {:?}", output.status));
            target_refresh_failure(&ctx, project_root, project_key, warning).await
        }
        Err(err) => {
            target_refresh_failure(&ctx, project_root, project_key, format!("{err:#}")).await
        }
    }
}

async fn target_refresh_failure(
    ctx: &ServiceContext,
    project_root: PathBuf,
    project_key: String,
    error: String,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    tracing::warn!(
        project_key,
        error = %error,
        "target local source refresh failed"
    );
    let source_id = local_source_id(&project_root);
    let committed_generation = ctx
        .target_local_source_runtime()
        .expect("target runtime checked before canonical source refresh")
        .ledger
        .committed_generation(source_id.clone())
        .await?;
    Ok(target_refresh_failed_result(
        project_root,
        project_key,
        Some(source_id),
        committed_generation,
        error,
    ))
}

fn target_refresh_unavailable_result(identity: CodeIndexIdentity) -> CodeSearchRefreshResult {
    target_refresh_failed_result(
        identity.project_root,
        identity.project_key,
        None,
        None,
        "target local source code-search refresh dependencies are not available".to_string(),
    )
}

fn target_refresh_failed_result(
    project_root: PathBuf,
    project_key: String,
    source_id: Option<SourceId>,
    committed_generation: Option<SourceGenerationId>,
    error: String,
) -> CodeSearchRefreshResult {
    CodeSearchRefreshResult {
        project_root,
        project_key,
        target_source_id: if committed_generation.is_some() {
            source_id
        } else {
            None
        },
        target_source_generation: committed_generation,
        freshness: code_search_freshness("stale", Some(FreshnessWarning::Failed { error }), 0, 0),
    }
}

fn emit_target_progress_started(progress: Option<&dyn ReindexProgressSink>) {
    if let Some(progress) = progress {
        progress.emit(ReindexProgress::Started {
            generation: 0,
            total_files: 0,
            added_files: 0,
            modified_files: 0,
            removed_files: 0,
            total_batches: 0,
        });
    }
}

fn emit_target_progress_finished(progress: Option<&dyn ReindexProgressSink>) {
    if let Some(progress) = progress {
        progress.emit(ReindexProgress::Finished { generation: 0 });
    }
}

pub(super) async fn target_code_search_committed_state_owned(
    ctx: ServiceContext,
    cwd: Option<PathBuf>,
    caller: CodeSearchCaller,
    collection: String,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    let root = resolve_code_search_root(cwd.as_deref(), caller).await?;
    let identity = code_search_identity(ctx.cfg().clone(), root, collection).await;
    let Some(target) = ctx.target_local_source_runtime() else {
        return Ok(target_refresh_unavailable_result(identity));
    };
    let source_id = local_source_id(&identity.project_root);
    let committed = target
        .ledger
        .committed_generation(source_id.clone())
        .await?;
    Ok(CodeSearchRefreshResult {
        project_root: identity.project_root,
        project_key: identity.project_key,
        target_source_id: committed.as_ref().map(|_| source_id),
        target_source_generation: committed,
        freshness: code_search_freshness("skipped", None, 0, 0),
    })
}
