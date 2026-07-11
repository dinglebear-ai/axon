use std::error::Error;
use std::path::{Path, PathBuf};

use axon_api::source::JobId;
use axon_api::source::{SourceGenerationId, SourceId};

use crate::context::ServiceContext;
use crate::local_source::{
    LocalSourceIndexInput, LocalSourceSelectionPolicy, index_local_source_with_job, local_source_id,
};
use crate::types::{CodeSearchCaller, CodeSearchFreshness};

use super::{
    CodeIndexIdentity, FreshnessWarning, ReindexProgress, ReindexProgressSink,
    code_search_freshness, code_search_identity, resolve_code_search_root,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CodeSearchRefreshResult {
    pub project_root: PathBuf,
    pub project_key: String,
    pub legacy_code_index_generation: Option<i64>,
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
    let identity = code_search_identity(ctx.cfg(), root).await;
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
    progress: Option<&dyn ReindexProgressSink>,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    refresh_target_local_code_search_index_with_progress(ctx, cwd, caller, progress).await
}

async fn refresh_target_local_code_search_index_with_progress(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
    progress: Option<&dyn ReindexProgressSink>,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    let root = resolve_code_search_root(cwd, caller).await?;
    let identity = code_search_identity(ctx.cfg(), root).await;
    let Some(target) = ctx.target_local_source_runtime() else {
        return Ok(target_refresh_unavailable_result(identity));
    };
    let project_root = identity.project_root.clone();
    let project_key = identity.project_key.clone();
    let input = LocalSourceIndexInput {
        root: identity.project_root.clone(),
        collection: ctx.cfg().collection.clone(),
        owner_id: format!("code-search:{}", identity.project_key),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        embedding_provider_id: target.embedding_provider_id.clone(),
        vector_provider_id: target.vector_provider_id.clone(),
        embedding_model: target.embedding_model.clone(),
        embedding_dimensions: target.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::CodeSearch,
        embedding_reservations: Some(target.embedding_reservations.clone()),
        vector_reservations: Some(target.vector_reservations.clone()),
        auth_snapshot: None,
        embed: true,
        route: None,
    };
    emit_target_progress_started(progress);
    match index_local_source_with_job(
        input,
        target.jobs.as_ref(),
        target.ledger.as_ref(),
        target.embedding_provider.as_ref(),
        target.vector_store.as_ref(),
    )
    .await
    {
        Ok(output) => {
            emit_target_progress_finished(progress);
            let result = CodeSearchRefreshResult {
                project_root: project_root.clone(),
                project_key: project_key.clone(),
                legacy_code_index_generation: None,
                target_source_id: Some(output.source_id),
                target_source_generation: Some(output.generation),
                freshness: code_search_freshness(
                    "fresh",
                    None,
                    usize::try_from(output.documents_prepared).unwrap_or(usize::MAX),
                    usize::try_from(output.removed_files).unwrap_or(usize::MAX),
                ),
            };
            tracing::debug!(
                project_key,
                indexed_files = result.freshness.indexed_files,
                "target local source refresh completed for code-search"
            );
            Ok(result)
        }
        Err(err) => {
            tracing::warn!(
                project_key,
                error = %err,
                "target local source refresh failed"
            );
            let source_id = local_source_id(&project_root);
            let committed_generation = target
                .ledger
                .committed_generation(source_id.clone())
                .await?;
            Ok(target_refresh_failed_result(
                project_root,
                project_key,
                Some(source_id),
                committed_generation,
                err.to_string(),
            ))
        }
    }
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
        legacy_code_index_generation: None,
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

pub(super) async fn target_code_search_committed_state(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
) -> Result<CodeSearchRefreshResult, Box<dyn Error + Send + Sync>> {
    let root = resolve_code_search_root(cwd, caller).await?;
    let identity = code_search_identity(ctx.cfg(), root).await;
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
        legacy_code_index_generation: None,
        target_source_id: committed.as_ref().map(|_| source_id),
        target_source_generation: committed,
        freshness: code_search_freshness("skipped", None, 0, 0),
    })
}
