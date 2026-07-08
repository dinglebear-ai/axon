//! `memory compact` — merge multiple memories into one, tracked as a
//! job-backed operation via [`super::job_tracking`].

use super::*;

pub async fn compact(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let request_json = json!({
        "operation": "memory_compaction",
        "memory_ids": req.memory_ids,
        "strategy": req.strategy,
    });
    job_tracking::track_operation_job(
        ctx,
        axon_api::source::OperationKind::MemoryCompaction,
        request_json,
        || compact_inner(ctx, req),
    )
    .await
}

async fn compact_inner(ctx: &ServiceContext, req: MemoryRequest) -> Result<MemoryItem> {
    let store = memory_store(ctx).await?;
    let memory_ids = req
        .memory_ids
        .clone()
        .filter(|ids| !ids.is_empty())
        .context("compact requires memory_ids (at least 2)")?;
    for id in &memory_ids {
        ensure_exists(store.as_ref(), id).await?;
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
    let result = store
        .compact(MemoryCompactRequest {
            memory_ids: memory_ids.into_iter().map(MemoryId::new).collect(),
            strategy,
            result_type,
            title: req.title.clone(),
            scope,
            archive_sources: req.archive_sources.unwrap_or(false),
            instructions: None,
            timestamp: Timestamp(SystemClock.now_rfc3339()),
        })
        .await
        .map_err(store_err)?;
    let record = store
        .get(result.memory_id.clone())
        .await
        .map_err(store_err)?
        .context("compacted memory not found after write")?;
    Ok(item_from_record(&record, Some(result.memory_score as f64)))
}
