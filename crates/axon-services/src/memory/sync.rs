//! Canonical publication handoff for durable memory mutations.
//!
//! SQLite commits first and remains authoritative. Each affected record is then
//! run inline through `memory://` when the data plane is attached, or represented
//! by a durable `JobKind::Source` row when this process is enqueue-only. A queued
//! source job is the recovery marker; if even that marker cannot be persisted,
//! the record receives a same-status history marker and the caller gets an error.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use axon_api::source::{
    AuthSnapshot, LifecycleStatus, MemoryId, MemoryRecord, MemoryStatusRequest, MetadataMap,
    SourceRefreshPolicy, SourceRequest, SourceScope, Timestamp,
};
use axon_memory::store::MemoryStore;
use sha2::{Digest, Sha256};

use crate::context::ServiceContext;

const SYNC_POLICY_VERSION: &str = "memory-source-sync-v1";
const INLINE_SYNC_LIMIT: usize = 16;
const ENQUEUE_BATCH_SIZE: usize = 64;

pub(crate) async fn sync_memory_records<I>(
    ctx: &ServiceContext,
    store: &dyn MemoryStore,
    memory_ids: I,
    operation: &str,
) -> Result<()>
where
    I: IntoIterator<Item = MemoryId> + Send,
    I::IntoIter: Send,
{
    let records = load_unique_records(store, memory_ids.into_iter().collect()).await?;
    if records.is_empty() {
        return Ok(());
    }

    if ctx.target_local_source_runtime().is_some() && records.len() <= INLINE_SYNC_LIMIT {
        let ctx = ctx.clone();
        let operation = operation.to_string();
        let mut outcomes = Vec::with_capacity(records.len());
        for record in records {
            let result = crate::source::index_source_with_auth(
                source_request(&record, &operation),
                &ctx,
                Some(AuthSnapshot::trusted_system(SYNC_POLICY_VERSION)),
            )
            .await;
            outcomes.push((record, result));
        }
        for (record, result) in outcomes {
            if matches!(
                result.as_ref().map(|result| result.status),
                Ok(LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded)
            ) {
                continue;
            }
            tracing::warn!(
                memory_id = %record.memory_id.0,
                operation,
                outcome = ?result,
                "memory canonical publication failed inline; enqueueing recovery"
            );
            enqueue_or_mark(&ctx, store, &record, &operation).await?;
        }
        return Ok(());
    }

    enqueue_or_mark_batch(ctx, store, &records, operation).await?;
    Ok(())
}

pub(crate) async fn enqueue_memory_records(
    job_store: &dyn axon_jobs::boundary::JobStore,
    records: &[MemoryRecord],
    operation: &str,
) -> Result<()> {
    for batch in records.chunks(ENQUEUE_BATCH_SIZE) {
        for record in batch {
            let result = crate::source::enqueue::enqueue_source(
                source_request(record, operation),
                job_store,
                Some(AuthSnapshot::trusted_system(SYNC_POLICY_VERSION)),
            )
            .await
            .with_context(|| format!("enqueue memory source sync for {}", record.memory_id.0))?;
            if !matches!(
                result.status,
                LifecycleStatus::Queued
                    | LifecycleStatus::Pending
                    | LifecycleStatus::Running
                    | LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
            ) {
                bail!(
                    "memory source sync for {} was not accepted: {:?}",
                    record.memory_id.0,
                    result.status
                );
            }
        }
    }
    Ok(())
}

async fn enqueue_or_mark_batch(
    ctx: &ServiceContext,
    memory_store: &dyn MemoryStore,
    records: &[MemoryRecord],
    operation: &str,
) -> Result<()> {
    let Some(job_store) = ctx.job_store() else {
        let error = anyhow::anyhow!("unified source job store is unavailable");
        for record in records {
            mark_sync_recovery(memory_store, record, operation, &error.to_string()).await?;
        }
        return Err(error);
    };
    if let Err(error) = enqueue_memory_records(job_store.as_ref(), records, operation).await {
        for record in records {
            mark_sync_recovery(memory_store, record, operation, &error.to_string()).await?;
        }
        return Err(
            error.context("memory records are durable in SQLite but publication is pending")
        );
    }
    ctx.notify_unified();
    Ok(())
}

async fn enqueue_or_mark(
    ctx: &ServiceContext,
    memory_store: &dyn MemoryStore,
    record: &MemoryRecord,
    operation: &str,
) -> Result<()> {
    let enqueue_result = match ctx.job_store() {
        Some(job_store) => {
            enqueue_memory_records(job_store.as_ref(), std::slice::from_ref(record), operation)
                .await
        }
        None => Err(anyhow::anyhow!("unified source job store is unavailable")),
    };
    match enqueue_result {
        Ok(()) => {
            ctx.notify_unified();
            Ok(())
        }
        Err(error) => {
            mark_sync_recovery(memory_store, record, operation, &error.to_string()).await?;
            Err(error.context(format!(
                "memory {} is durable in SQLite but canonical publication is pending",
                record.memory_id.0
            )))
        }
    }
}

async fn load_unique_records(
    store: &dyn MemoryStore,
    memory_ids: Vec<MemoryId>,
) -> Result<Vec<MemoryRecord>> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for memory_id in memory_ids {
        if !seen.insert(memory_id.0.clone()) {
            continue;
        }
        let record = store
            .get(memory_id.clone())
            .await
            .map_err(super::store_err)?
            .with_context(|| format!("memory {} missing after mutation", memory_id.0))?;
        records.push(record);
    }
    Ok(records)
}

fn source_request(record: &MemoryRecord, operation: &str) -> SourceRequest {
    let mut metadata = MetadataMap::new();
    metadata.insert("memory_mutation".to_string(), serde_json::json!(operation));
    metadata.insert(
        "memory_recovery_marker".to_string(),
        serde_json::json!(true),
    );
    let mut request = SourceRequest::new(format!("memory://{}", record.memory_id.0))
        .with_refresh(SourceRefreshPolicy::Force);
    request.scope = Some(SourceScope::Api);
    request.adapter = Some("memory".to_string());
    request.metadata = metadata;
    request.idempotency_key = Some(format!(
        "memory-source-sync:{}:{}:{}",
        operation,
        record.memory_id.0,
        record_version(record)
    ));
    request
}

fn record_version(record: &MemoryRecord) -> String {
    let encoded = serde_json::to_vec(record).unwrap_or_else(|_| record.body.as_bytes().to_vec());
    format!("{:x}", Sha256::digest(encoded))
}

pub(crate) async fn mark_sync_recovery(
    store: &dyn MemoryStore,
    record: &MemoryRecord,
    operation: &str,
    error: &str,
) -> Result<()> {
    store
        .set_status(MemoryStatusRequest {
            memory_id: record.memory_id.clone(),
            status: record.status,
            reason: Some(format!(
                "memory.source_sync_pending operation={operation}: {error}"
            )),
            timestamp: Timestamp::from(chrono::Utc::now()),
        })
        .await
        .map_err(super::store_err)?;
    Ok(())
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
