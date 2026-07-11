//! `axon memory` command path, backed by the real `axon-memory`
//! `SqliteMemoryStore` on the unified SQLite pool (#298).
//!
//! This module is the transport-facing facade: it keeps the CLI/MCP/REST
//! `memory` surface (subactions remember/list/search/show/link/supersede/
//! context and the `MemoryItem`/`MemoryEdgeItem`/`MemoryContext` output shapes)
//! and routes every operation through [`axon_memory::sqlite::SqliteMemoryStore`]
//! with vector-backed recall when the target runtime has Qdrant/TEI providers
//! attached. SQLite remains the metadata source of truth (`memory_records`/
//! `memory_links`/…), while Qdrant points live in `vector_namespace=memory`.
//! Pure DTO/mapping helpers live in [`mapping`]; this file owns dispatch +
//! store composition.

use anyhow::{Context, Result, bail};
use serde_json::json;
use std::sync::Arc;

use crate::context::ServiceContext;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{MemoryEdgeType, MemoryRequest, MemorySubaction};
use axon_api::source::{
    MemoryArchiveRequest, MemoryCompactRequest, MemoryContradictRequest, MemoryForgetRequest,
    MemoryId, MemoryLink, MemoryPinRequest, MemoryReinforcement, MemoryReviewRequest, MemoryScope,
    MemorySearchRequest, MemorySupersedeRequest, Timestamp,
};
use axon_graph::SqliteGraphStore;
use axon_memory::graph::{GraphBackedMemoryMirror, GraphBackedMemoryStore};
use axon_memory::record::{Clock, SystemClock};
use axon_memory::sqlite::SqliteMemoryStore;
use axon_memory::store::MemoryStore;
use axon_memory::vector::{MemoryBatchLimits, MemoryVectorConfig, VectorBackedMemoryStore};

mod compact;
mod context_format;
pub(crate) mod import_export;
mod job_tracking;
mod mapping;
mod runtime_metadata;
mod store;

pub use compact::compact;
pub use import_export::{MAX_MEMORY_IMPORT_RECORDS, MemoryAuthz, export, import};
// `pub(crate)` (not just `use`) so `crate::source::open_cleanup_debt_stores`
// can open the same composed (graph-mirrored, vector-backed) memory store
// this module's own subactions use, instead of a bare `SqliteMemoryStore`
// that would skip that composition's extra cleanup on `forget()`.
pub(crate) use store::memory_store;
#[cfg(test)]
mod tests;

use context_format::format_memory_context;
use mapping::{
    MemoryContext, MemoryEdgeItem, MemoryItem, edge_type_name, facet_links, facet_matches,
    item_from_record, node_type_name, normalize_remember, parse_memory_type, required_text,
    scope_for, status_matches,
};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 100;
const DEFAULT_CONTEXT_TOKEN_BUDGET: usize = 2_000;
const MAX_CONTEXT_TOKEN_BUDGET: usize = 16_000;

/// Dispatch a flat CLI/MCP `memory` subaction.
///
/// `authz` gates [`MemorySubaction::Import`] when the request's `import_mode`
/// is `replace_scope` (see [`MemoryAuthz`]). Every other subaction ignores
/// it. Callers without a meaningful auth boundary (CLI: local-trust; MCP
/// `LoopbackDev`) should pass [`MemoryAuthz::admin`]; transports with a real
/// caller identity (MCP `Mounted`, REST) must derive it from the caller's
/// resolved scopes.
pub async fn dispatch(
    ctx: &ServiceContext,
    req: MemoryRequest,
    authz: &MemoryAuthz,
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
        MemorySubaction::Reinforce => {
            let item = reinforce(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Contradict => {
            let edge = contradict(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "edge": edge }))
        }
        MemorySubaction::Pin => {
            let item = pin(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Archive => {
            let item = archive(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Forget => {
            let item = forget(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Review => {
            let items = review(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memories": items }))
        }
        MemorySubaction::Compact => {
            let item = compact(ctx, req).await.map_err(memory_error)?;
            Ok(json!({ "memory": item }))
        }
        MemorySubaction::Import => {
            let import_req = import_export::import_request_from_flat(req).map_err(memory_error)?;
            let result = import(ctx, import_req, authz).await.map_err(memory_error)?;
            Ok(serde_json::to_value(result).unwrap_or(json!({})))
        }
        MemorySubaction::Export => {
            let result = export(ctx, import_export::export_request_from_flat(req), authz)
                .await
                .map_err(memory_error)?;
            Ok(serde_json::to_value(result).unwrap_or(json!({})))
        }
    }
}

pub async fn list(ctx: &ServiceContext, req: MemoryRequest) -> Result<Vec<MemoryItem>> {
    let store = memory_store(ctx).await?;
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    // Empty-query search returns the full recall-visible set; then facet-filter.
    let search = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: String::new(),
            limit: MAX_LIMIT as u32,
            filters: Default::default(),
            include_graph: false,
            include_archived: req.status.as_deref() != Some("active"),
            reinforce: false,
        })
        .await
        .map_err(store_err)?;
    let want_type = req.memory_type.map(node_type_name);
    let mut items: Vec<MemoryItem> = search
        .results
        .into_iter()
        .map(|m| item_from_record(&m.record, Some(m.score as f64)))
        .filter(|item| status_matches(item, req.status.as_deref()))
        .filter(|item| facet_matches(item, req.project.as_deref(), "project"))
        .filter(|item| facet_matches(item, req.repo.as_deref(), "repo"))
        .filter(|item| facet_matches(item, req.file.as_deref(), "file"))
        .filter(|item| want_type.is_none_or(|t| item.memory_type == t))
        .collect();
    items.truncate(limit);
    Ok(items)
}

pub async fn remember(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let memory = normalize_remember(req)?;
    let request = axon_api::source::MemoryRequest {
        memory_type: parse_memory_type(&memory.memory_type),
        body: memory.body.clone(),
        confidence: memory.confidence as f32,
        salience: 0.5,
        scope: scope_for(&memory),
        title: Some(memory.title.clone()),
        tags: Vec::new(),
        links: facet_links(&memory),
        decay: None,
        embed: true,
        visibility: None,
    };
    let result = store.remember(request).await.map_err(store_err)?;
    let record = store
        .get(result.memory_id.clone())
        .await
        .map_err(store_err)?
        .context("remembered memory not found after write")?;
    Ok(item_from_record(&record, Some(result.memory_score as f64)))
}

pub async fn search(ctx: &ServiceContext, req: MemoryRequest) -> Result<Vec<MemoryItem>> {
    let query = required_text(req.query.as_deref(), "query")?.to_string();
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let store = memory_store(ctx).await?;
    let search = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query,
            limit: MAX_LIMIT as u32,
            filters: Default::default(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .map_err(store_err)?;
    let mut items: Vec<MemoryItem> = search
        .results
        .into_iter()
        .map(|m| item_from_record(&m.record, Some(m.score as f64)))
        .filter(|item| facet_matches(item, req.project.as_deref(), "project"))
        .filter(|item| facet_matches(item, req.repo.as_deref(), "repo"))
        .filter(|item| facet_matches(item, req.file.as_deref(), "file"))
        .collect();
    items.truncate(limit);
    Ok(items)
}

pub async fn show(ctx: &ServiceContext, req: MemoryRequest) -> Result<Option<MemoryItem>> {
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    let store = memory_store(ctx).await?;
    let record = match store.get(MemoryId::new(id)).await.map_err(store_err)? {
        Some(record) => record,
        None => return Ok(None),
    };
    Ok(Some(item_from_record(&record, None)))
}

pub async fn link(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryEdgeItem> {
    let store = memory_store(ctx).await?;
    let source_id = required_text(req.source_id.as_deref(), "source_id")?.to_string();
    let target_id = required_text(req.target_id.as_deref(), "target_id")?.to_string();
    let edge_type = edge_type_name(req.edge_type.unwrap_or(MemoryEdgeType::RelatesTo));
    // Attach an evidence-free link on the source memory pointing at the target.
    ensure_exists(store.as_ref(), &source_id).await?;
    ensure_exists(store.as_ref(), &target_id).await?;
    store
        .link(axon_api::source::MemoryLinkRequest {
            memory_id: MemoryId::new(source_id.clone()),
            link: MemoryLink {
                link_type: edge_type.to_string(),
                target: target_id.clone(),
                confidence: 1.0,
                evidence: Vec::new(),
            },
        })
        .await
        .map_err(store_err)?;
    Ok(edge_item(&source_id, &target_id, edge_type))
}

pub async fn supersede(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryEdgeItem> {
    let store = memory_store(ctx).await?;
    let replacement_id = required_text(req.source_id.as_deref(), "source_id")?.to_string();
    let superseded_id = required_text(req.target_id.as_deref(), "target_id")?.to_string();
    ensure_exists(store.as_ref(), &replacement_id).await?;
    ensure_exists(store.as_ref(), &superseded_id).await?;
    store
        .supersede(MemorySupersedeRequest {
            memory_id: MemoryId::new(superseded_id.clone()),
            replacement_id: MemoryId::new(replacement_id.clone()),
            reason: None,
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    Ok(edge_item(&replacement_id, &superseded_id, "supersedes"))
}

pub async fn context(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryContext> {
    let store = memory_store(ctx).await?;
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let token_budget = req
        .token_budget
        .unwrap_or(DEFAULT_CONTEXT_TOKEN_BUDGET)
        .clamp(1, MAX_CONTEXT_TOKEN_BUDGET);
    // Seed from a keyword search when a query is present, else the full set.
    let search = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: req.query.clone().unwrap_or_default(),
            limit: MAX_LIMIT as u32,
            filters: Default::default(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .map_err(store_err)?;
    let mut items: Vec<MemoryItem> = search
        .results
        .into_iter()
        .map(|m| item_from_record(&m.record, Some(m.score as f64)))
        .filter(|item| facet_matches(item, req.project.as_deref(), "project"))
        .filter(|item| facet_matches(item, req.repo.as_deref(), "repo"))
        .filter(|item| facet_matches(item, req.file.as_deref(), "file"))
        .collect();
    items.truncate(limit);
    Ok(format_memory_context(items, token_budget))
}

pub async fn reinforce(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    let amount = req.amount.unwrap_or(0.1) as f32;
    let reason = req
        .reason
        .clone()
        .unwrap_or_else(|| "reinforced".to_string());
    let result = store
        .reinforce(
            MemoryId::new(id.clone()),
            MemoryReinforcement {
                amount,
                reason,
                timestamp: Timestamp(SystemClock.now_rfc3339()),
            },
        )
        .await
        .map_err(store_err)?;
    let record = store
        .get(MemoryId::new(id))
        .await
        .map_err(store_err)?
        .context("reinforced memory not found after write")?;
    Ok(item_from_record(&record, Some(result.memory_score as f64)))
}

pub async fn contradict(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryEdgeItem> {
    let store = memory_store(ctx).await?;
    let memory_id = required_text(req.source_id.as_deref(), "source_id")?.to_string();
    let conflicting_id = required_text(req.target_id.as_deref(), "target_id")?.to_string();
    ensure_exists(store.as_ref(), &memory_id).await?;
    ensure_exists(store.as_ref(), &conflicting_id).await?;
    store
        .contradict(MemoryContradictRequest {
            memory_id: MemoryId::new(memory_id.clone()),
            conflicting_id: MemoryId::new(conflicting_id.clone()),
            reason: req.reason.clone(),
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    Ok(edge_item(&memory_id, &conflicting_id, "contradicts"))
}

pub async fn pin(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    ensure_exists(store.as_ref(), &id).await?;
    store
        .pin(MemoryPinRequest {
            memory_id: MemoryId::new(id.clone()),
            pinned: req.pinned.unwrap_or(true),
            reason: req.reason.clone(),
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    let record = store
        .get(MemoryId::new(id))
        .await
        .map_err(store_err)?
        .context("pinned memory not found after write")?;
    Ok(item_from_record(&record, None))
}

pub async fn archive(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    ensure_exists(store.as_ref(), &id).await?;
    store
        .archive(MemoryArchiveRequest {
            memory_id: MemoryId::new(id.clone()),
            reason: req.reason.clone(),
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    let record = store
        .get(MemoryId::new(id))
        .await
        .map_err(store_err)?
        .context("archived memory not found after write")?;
    Ok(item_from_record(&record, None))
}

pub async fn forget(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    ensure_exists(store.as_ref(), &id).await?;
    store
        .forget(MemoryForgetRequest {
            memory_id: MemoryId::new(id.clone()),
            reason: req.reason.clone(),
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    // Forgotten memories return no body content — same visibility rule as
    // the transport layer applies for a `forgotten` status memory anywhere
    // else it's surfaced.
    let mut record = store
        .get(MemoryId::new(id))
        .await
        .map_err(store_err)?
        .context("forgotten memory not found after write")?;
    record.body.clear();
    Ok(item_from_record(&record, None))
}

pub async fn review(ctx: &ServiceContext, req: MemoryRequest) -> Result<Vec<MemoryItem>> {
    let store = memory_store(ctx).await?;
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as u32;
    let result = store
        .review(MemoryReviewRequest {
            reason: req.reason.clone(),
            memory_type: req
                .memory_type
                .map(|t| parse_memory_type(node_type_name(t))),
            scope: None,
            limit: Some(limit),
            cursor: None,
        })
        .await
        .map_err(store_err)?;
    Ok(result
        .memories
        .iter()
        .map(|record| item_from_record(record, None))
        .collect())
}

async fn ensure_exists(store: &dyn MemoryStore, id: &str) -> Result<()> {
    if store
        .get(MemoryId::new(id.to_string()))
        .await
        .map_err(store_err)?
        .is_none()
    {
        bail!("memory not found: {id}");
    }
    Ok(())
}

/// Build the CLI edge DTO for a link/supersede result.
fn edge_item(source_id: &str, target_id: &str, edge_type: &str) -> MemoryEdgeItem {
    let now = SystemClock.now_epoch_secs() * 1_000;
    MemoryEdgeItem {
        id: format!("{source_id}|{target_id}|{edge_type}"),
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        edge_type: edge_type.to_string(),
        created_at: now,
        updated_at: now,
    }
}

/// Convert a memory-store `ApiError` into the crate-wide `anyhow` error path.
fn store_err(err: axon_api::source::ApiError) -> anyhow::Error {
    anyhow::anyhow!("{}", err.message)
}

fn memory_error(err: anyhow::Error) -> ClientActionError {
    ClientActionError::new("memory_error", err.to_string(), false, None)
}
