//! Status-transition and edit lifecycle operations split out of the parent
//! `memory` module to stay under the repo's monolith line cap: reinforce,
//! contradict, pin, archive, forget, update, review, plus their shared
//! `ensure_exists`/`edge_item` helpers.

use anyhow::{Context, Result, bail};

use super::mapping::{
    MemoryEdgeItem, MemoryItem, item_from_record, node_type_name, parse_memory_type, required_text,
};
use super::store::memory_store;
use super::{DEFAULT_LIMIT, MAX_LIMIT, store_err};
use crate::context::ServiceContext;
use axon_api::mcp_schema::MemoryRequest;
use axon_api::source::{
    MemoryArchiveRequest, MemoryContradictRequest, MemoryForgetRequest, MemoryId, MemoryPinRequest,
    MemoryReinforcement, MemoryReviewRequest, MemoryScope,
};
use axon_memory::record::{Clock, SystemClock};
use axon_memory::store::MemoryStore;

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
                timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
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
            timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
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
            timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
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
            timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
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
            timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
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

/// Edit a memory's editable fields in place. REST: `PATCH
/// /v1/memories/{memory_id}`.
pub async fn update(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let id = required_text(req.id.as_deref(), "id")?.to_string();
    ensure_exists(store.as_ref(), &id).await?;
    let scope = match (req.scope_kind.clone(), req.scope_value.clone()) {
        (Some(kind), Some(value)) => Some(MemoryScope { kind, value }),
        (None, None) => None,
        _ => bail!("scope_kind and scope_value must be supplied together"),
    };
    store
        .update(axon_api::source::MemoryUpdateRequest {
            memory_id: MemoryId::new(id.clone()),
            body: req.body.clone(),
            title: req.title.clone(),
            memory_type: req
                .memory_type
                .map(|t| parse_memory_type(node_type_name(t))),
            confidence: req.confidence.map(|c| c as f32),
            salience: req.salience.map(|s| s as f32),
            scope,
            reason: req.reason.clone(),
            timestamp: axon_api::source::Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    let record = store
        .get(MemoryId::new(id))
        .await
        .map_err(store_err)?
        .context("updated memory not found after write")?;
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

pub(super) async fn ensure_exists(store: &dyn MemoryStore, id: &str) -> Result<()> {
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
pub(super) fn edge_item(source_id: &str, target_id: &str, edge_type: &str) -> MemoryEdgeItem {
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
