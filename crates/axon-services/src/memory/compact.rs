//! `memory compact` — merge multiple memories into one, tracked as a
//! job-backed operation via [`super::job_tracking`].

use super::*;
use axon_api::source::MemoryScope;

/// Build the typed [`MemoryCompactRequest`] from the flat CLI/MCP
/// [`MemoryRequest`] shape. Split out from [`compact`] so the fully-built
/// domain request can be embedded verbatim in the tracked job's
/// `request_json` (`"payload"` field) — the detached unified-worker runner
/// (`crate::runtime::job_runners::MemoryCompactionRunner`) deserializes that
/// same shape to execute a `memory_compaction` job it claims independently
/// of this foreground call.
pub(super) async fn build_compact_request(
    store: &dyn MemoryStore,
    req: &MemoryRequest,
) -> Result<MemoryCompactRequest> {
    let memory_ids = req
        .memory_ids
        .clone()
        .filter(|ids| !ids.is_empty())
        .context("compact requires memory_ids (at least 2)")?;
    for id in &memory_ids {
        ensure_exists(store, id).await?;
    }
    let strategy = req
        .strategy
        .clone()
        .unwrap_or_else(|| "concatenate".to_string());
    let result_type = req
        .memory_type
        .map(|t| parse_memory_type(node_type_name(t)))
        .unwrap_or(axon_api::source::MemoryType::Fact);
    let scope = if let Some(project) = req.project.clone() {
        MemoryScope {
            kind: "project".to_string(),
            value: project,
        }
    } else if let Some(repo) = req.repo.clone() {
        MemoryScope {
            kind: "repo".to_string(),
            value: repo,
        }
    } else if let Some(file) = req.file.clone() {
        MemoryScope {
            kind: "file".to_string(),
            value: file,
        }
    } else {
        MemoryScope {
            kind: "global".to_string(),
            value: String::new(),
        }
    };
    Ok(MemoryCompactRequest {
        memory_ids: memory_ids.into_iter().map(MemoryId::new).collect(),
        strategy,
        result_type,
        title: req.title.clone(),
        scope,
        archive_sources: req.archive_sources.unwrap_or(false),
        instructions: None,
        timestamp: Timestamp(SystemClock.now_rfc3339()),
    })
}

pub async fn compact(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let request = build_compact_request(store.as_ref(), &req).await?;
    let request_json = json!({
        "operation": "memory_compaction",
        "payload": serde_json::to_value(&request).context("serialize compact request")?,
    });
    job_tracking::track_operation_job(
        ctx,
        axon_api::source::OperationKind::MemoryCompaction,
        request_json,
        || compact_with_store(ctx, store, request),
    )
    .await
}

async fn compact_with_store(
    ctx: &ServiceContext,
    store: Arc<dyn MemoryStore>,
    request: MemoryCompactRequest,
) -> Result<MemoryItem> {
    let archived_source_ids = if request.archive_sources {
        request.memory_ids.clone()
    } else {
        Default::default()
    };
    let result = store.compact(request).await.map_err(store_err)?;
    let mut sync_ids = vec![result.memory_id.clone()];
    sync_ids.extend(archived_source_ids);
    super::sync::sync_memory_records(ctx, store.as_ref(), sync_ids, "compact").await?;
    let record = store
        .get(result.memory_id.clone())
        .await
        .map_err(store_err)?
        .context("compacted memory not found after write")?;
    Ok(item_from_record(&record, Some(result.memory_score as f64)))
}
