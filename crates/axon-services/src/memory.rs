use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::context::ServiceContext;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{MemoryEdgeType, MemoryNodeType, MemoryRequest, MemorySubaction};
use axon_core::config::{Config, ConfigOverrides};
use axon_core::logging::log_warn;
use axon_ingest::sessions::redact_session_text;
use axon_jobs::store::now_ms;
use axon_vector::ops::qdrant::{
    qdrant_delete_by_url_filter, qdrant_hybrid_search, qdrant_named_dense_search,
};
use axon_vector::ops::sparse::compute_sparse_vector;
use axon_vector::ops::tei::{EmbedInput, embed_prepared_docs, tei_embed_typed};
use axon_vector::ops::{SourceDocument, prepare_source_document};

mod context_format;
mod runtime_metadata;
mod store;
#[cfg(test)]
mod tests;
mod vector_store;

use context_format::format_memory_context;
use runtime_metadata::detect_runtime_memory_metadata;
use store::{
    bump_access, context_seed_nodes, link_nodes, list_nodes, node_by_id, node_by_id_optional,
    supersede_node, upsert_node,
};
use vector_store::{
    hydrate_memory_bodies, memory_filter, retrieve_body_by_id, update_qdrant_memory_status,
};

const DEFAULT_MEMORY_COLLECTION: &str = "axon_memory";
const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 100;
const DEFAULT_CONTEXT_TOKEN_BUDGET: usize = 2_000;
const MAX_CONTEXT_TOKEN_BUDGET: usize = 16_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub id: String,
    pub memory_type: String,
    pub title: String,
    pub body: Option<String>,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub file: Option<String>,
    pub workspace: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub cwd: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_seen_at: i64,
    pub access_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEdgeItem {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryContext {
    pub context: String,
    pub memories: Vec<MemoryItem>,
    pub token_budget: usize,
    pub token_estimate: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
struct NormalizedMemory {
    id: Uuid,
    memory_type: String,
    title: String,
    body: String,
    project: Option<String>,
    repo: Option<String>,
    file: Option<String>,
    workspace: Option<String>,
    git_branch: Option<String>,
    git_commit: Option<String>,
    git_dirty: Option<bool>,
    cwd: Option<String>,
    confidence: f64,
}

pub async fn dispatch(
    ctx: &ServiceContext,
    req: MemoryRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(MemorySubaction::Remember) {
        MemorySubaction::Remember => {
            let item = remember(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::List => {
            let items = list(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memories": items }))
        }
        MemorySubaction::Search => {
            let items = search(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memories": items }))
        }
        MemorySubaction::Show => {
            let item = show(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Link => {
            let edge = link(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "edge": edge }))
        }
        MemorySubaction::Supersede => {
            let edge = supersede(ctx, req).await.map_err(memory_error)?;
            Ok(json!({
                "edge": edge,
                "superseded_id": edge.target_id,
                "replacement_id": edge.source_id
            }))
        }
        MemorySubaction::Context => {
            let context = context(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "context": context }))
        }
    }
}

pub async fn list(ctx: &ServiceContext, req: MemoryRequest) -> Result<Vec<MemoryItem>> {
    let pool = memory_pool(ctx)?;
    list_nodes(
        &pool,
        req.project.as_deref(),
        req.repo.as_deref(),
        req.file.as_deref(),
        req.memory_type.map(node_type_name),
        req.status.as_deref(),
        req.limit.unwrap_or(DEFAULT_LIMIT),
    )
    .await
}

pub async fn remember(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let pool = memory_pool(ctx)?;
    let memory = normalize_remember(req)?;
    let cfg = memory_config(ctx.cfg());
    let text = format!("{}\n\n{}", memory.title, memory.body);
    let now = now_ms();
    let url = format!("memory://{}", memory.id);
    let payload = json!({
        "memory": true,
        "type": memory.memory_type,
        "title": memory.title,
        "body": memory.body,
        "project": memory.project,
        "repo": memory.repo,
        "file": memory.file,
        "workspace": memory.workspace,
        "git_branch": memory.git_branch,
        "git_commit": memory.git_commit,
        "git_dirty": memory.git_dirty,
        "cwd": memory.cwd,
        "confidence": memory.confidence,
        "status": "active",
        "source": "manual",
        "created_at": now,
        "updated_at": now,
        "last_seen_at": now,
        "access_count": 0,
        "text": text,
    });
    let source = SourceDocument::new_memory(
        url.clone(),
        text,
        Some(memory.title.clone()),
        Some(payload),
        memory.id,
    );
    let doc = prepare_source_document(source)
        .await
        .map_err(|err| anyhow!("prepare memory source failed: {err}"))?;
    let summary = embed_prepared_docs(&cfg, vec![doc], None)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    if summary.docs_failed > 0 {
        bail!("memory embed failed");
    }
    if let Err(err) = upsert_node(&pool, &memory, now).await {
        if let Err(cleanup_err) = qdrant_delete_by_url_filter(&cfg, &url).await {
            log_warn(&format!(
                "memory qdrant cleanup failed after sqlite write error url={url} err={cleanup_err}"
            ));
        }
        return Err(err);
    }
    let mut item = node_by_id(&pool, &memory.id.to_string()).await?;
    item.body = Some(memory.body);
    Ok(item)
}

pub async fn search(ctx: &ServiceContext, req: MemoryRequest) -> Result<Vec<MemoryItem>> {
    let query = required_text(req.query.as_deref(), "query")?;
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let cfg = memory_config(ctx.cfg());
    let dense = tei_embed_typed(&cfg, &[EmbedInput::query(query.to_string())])
        .await
        .map_err(|e| anyhow!(e.to_string()))?
        .into_iter()
        .next()
        .context("TEI returned no query embedding for memory search")?;
    let filter = memory_filter(
        req.project.as_deref(),
        req.repo.as_deref(),
        req.file.as_deref(),
    );
    let sparse = compute_sparse_vector(query);
    let hits = if sparse.is_empty() {
        qdrant_named_dense_search(&cfg, &dense, limit, Some(&filter)).await?
    } else {
        qdrant_hybrid_search(&cfg, &dense, &sparse, limit, None, Some(&filter)).await?
    };
    let pool = memory_pool(ctx)?;
    let ids = hits
        .iter()
        .filter_map(|hit| hit.id.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    bump_access(&pool, &ids).await?;
    let mut items = Vec::new();
    for hit in hits {
        let payload = hit.payload;
        items.push(MemoryItem {
            id: hit
                .id
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| hit.id.to_string()),
            memory_type: payload.memory_type.unwrap_or_else(|| "fact".to_string()),
            title: payload.memory_title.unwrap_or_default(),
            body: payload.memory_body,
            project: payload.memory_project,
            repo: payload.memory_repo,
            file: payload.memory_file,
            workspace: payload.memory_workspace,
            git_branch: payload.memory_git_branch,
            git_commit: payload.memory_git_commit,
            git_dirty: payload.memory_git_dirty,
            cwd: payload.memory_cwd,
            confidence: payload.memory_confidence.unwrap_or(1.0),
            status: payload
                .memory_status
                .unwrap_or_else(|| "active".to_string()),
            created_at: payload.memory_created_at.unwrap_or_default(),
            updated_at: payload.memory_updated_at.unwrap_or_default(),
            last_seen_at: now_ms(),
            access_count: payload.memory_access_count.unwrap_or_default() + 1,
            score: Some(hit.score),
        });
    }
    Ok(items)
}

pub async fn show(ctx: &ServiceContext, req: MemoryRequest) -> Result<Option<MemoryItem>> {
    let id = required_text(req.id.as_deref(), "id")?;
    let pool = memory_pool(ctx)?;
    let mut item = match node_by_id_optional(&pool, id).await? {
        Some(item) => item,
        None => return Ok(None),
    };
    let cfg = memory_config(ctx.cfg());
    item.body = retrieve_body_by_id(&cfg, id).await?;
    bump_access(&pool, &[id.to_string()]).await?;
    item.access_count += 1;
    item.last_seen_at = now_ms();
    Ok(Some(item))
}

pub async fn link(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryEdgeItem> {
    let pool = memory_pool(ctx)?;
    let source_id = required_text(req.source_id.as_deref(), "source_id")?;
    let target_id = required_text(req.target_id.as_deref(), "target_id")?;
    let edge_type = edge_type_name(req.edge_type.unwrap_or(MemoryEdgeType::RelatesTo));
    link_nodes(&pool, source_id, target_id, edge_type, now_ms()).await
}

pub async fn supersede(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryEdgeItem> {
    let pool = memory_pool(ctx)?;
    let replacement_id = required_text(req.source_id.as_deref(), "source_id")?;
    let superseded_id = required_text(req.target_id.as_deref(), "target_id")?;
    if node_by_id_optional(&pool, replacement_id).await?.is_none() {
        bail!("memory not found: {replacement_id}");
    }
    if node_by_id_optional(&pool, superseded_id).await?.is_none() {
        bail!("memory not found: {superseded_id}");
    }
    let now = now_ms();
    let cfg = memory_config(ctx.cfg());
    update_qdrant_memory_status(&cfg, superseded_id, "superseded", now).await?;
    match supersede_node(&pool, replacement_id, superseded_id, now).await {
        Ok(edge) => Ok(edge),
        Err(err) => {
            if let Err(cleanup_err) =
                update_qdrant_memory_status(&cfg, superseded_id, "active", now).await
            {
                log_warn(&format!(
                    "memory qdrant supersede rollback failed id={superseded_id} err={cleanup_err}"
                ));
            }
            Err(err)
        }
    }
}

pub async fn context(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryContext> {
    let pool = memory_pool(ctx)?;
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let token_budget = req
        .token_budget
        .unwrap_or(DEFAULT_CONTEXT_TOKEN_BUDGET)
        .clamp(1, MAX_CONTEXT_TOKEN_BUDGET);
    let _depth = req.depth.unwrap_or(1).min(1);
    let seed_ids = if req.query.as_deref().is_some_and(|q| !q.trim().is_empty()) {
        search(ctx, req.clone())
            .await?
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut items = context_seed_nodes(
        &pool,
        req.project.as_deref(),
        req.repo.as_deref(),
        req.file.as_deref(),
        &seed_ids,
        limit,
    )
    .await?;
    hydrate_memory_bodies(&memory_config(ctx.cfg()), &mut items).await?;
    Ok(format_memory_context(items, token_budget))
}

pub fn memory_config(cfg: &Config) -> Config {
    cfg.apply_overrides(&ConfigOverrides {
        collection: Some(memory_collection_name()),
        ..ConfigOverrides::default()
    })
}

fn memory_collection_name() -> String {
    std::env::var("AXON_MEMORY_COLLECTION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_MEMORY_COLLECTION.to_string())
}

fn memory_pool(ctx: &ServiceContext) -> Result<Arc<SqlitePool>> {
    ctx.jobs
        .sqlite_pool()
        .context("memory requires a ServiceContext backed by SQLite")
}

fn normalize_remember(req: MemoryRequest) -> Result<NormalizedMemory> {
    if req.id.is_some() {
        bail!("id is not accepted for memory.remember; ids are server-generated");
    }
    let body = redact_session_text(required_text(req.body.as_deref(), "body")?);
    let title = req
        .title
        .as_deref()
        .map(redact_session_text)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| derive_title(&body));
    let memory_type = node_type_name(req.memory_type.unwrap_or(MemoryNodeType::Fact)).to_string();
    let confidence = req.confidence.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be between 0.0 and 1.0");
    }
    let runtime = detect_runtime_memory_metadata();
    let project = clean_opt(req.project).or(runtime.project);
    let repo = clean_opt(req.repo).or(runtime.repo);
    let file = clean_opt(req.file);
    let id = memory_id(
        &memory_type,
        project.as_deref(),
        repo.as_deref(),
        file.as_deref(),
        &title,
    );
    Ok(NormalizedMemory {
        id,
        memory_type,
        title,
        body,
        project,
        repo,
        file,
        workspace: runtime.workspace,
        git_branch: runtime.git_branch,
        git_commit: runtime.git_commit,
        git_dirty: runtime.git_dirty,
        cwd: runtime.cwd,
        confidence,
    })
}

fn required_text<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{field} is required"))
}

fn clean_opt(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn derive_title(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled memory")
        .chars()
        .take(120)
        .collect()
}

fn memory_id(
    memory_type: &str,
    project: Option<&str>,
    repo: Option<&str>,
    file: Option<&str>,
    title: &str,
) -> Uuid {
    let key = [
        memory_type,
        project.unwrap_or(""),
        repo.unwrap_or(""),
        file.unwrap_or(""),
        title,
    ]
    .map(canonical_part)
    .join("|");
    Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes())
}

fn canonical_part(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn node_type_name(value: MemoryNodeType) -> &'static str {
    match value {
        MemoryNodeType::Decision => "decision",
        MemoryNodeType::Fact => "fact",
        MemoryNodeType::Preference => "preference",
        MemoryNodeType::Task => "task",
        MemoryNodeType::Bug => "bug",
    }
}

fn edge_type_name(value: MemoryEdgeType) -> &'static str {
    match value {
        MemoryEdgeType::RelatesTo => "relates_to",
        MemoryEdgeType::Supersedes => "supersedes",
    }
}

fn edge_id(source_id: &str, target_id: &str, edge_type: &str) -> Uuid {
    let key = [source_id, target_id, edge_type]
        .map(canonical_part)
        .join("|");
    Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes())
}

fn memory_error(err: anyhow::Error) -> ClientActionError {
    ClientActionError::new("memory_error", err.to_string(), false, None)
}
