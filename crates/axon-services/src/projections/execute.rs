use std::sync::Arc;

use axon_api::source::*;
use axon_api::{CodeSearchCaller, CodeSearchOptions, QueryResult};
use axon_error::{ApiError, ErrorStage};
use axon_jobs::boundary::JobStore;
use sha2::{Digest, Sha256};

use super::{PreparedCodeSearchItem, PreparedSourceItem, ProjectionPreflight};
use crate::context::ServiceContext;

pub async fn enqueue_source_projection_batch(
    ctx: &ServiceContext,
    operation: ProjectionOperation,
    preflight: ProjectionPreflight<PreparedSourceItem>,
    auth: Option<AuthSnapshot>,
) -> Result<BatchResult<SourceResult>, ApiError> {
    let store = ctx.job_store().ok_or_else(|| {
        execution_error(
            "projection.job_store_unavailable",
            "projection admission requires the unified job store",
        )
    })?;
    admit_source_batch(store, operation, preflight, auth).await
}

/// Mutating foreground requests use the same durable admission boundary. The
/// canonical worker remains the sole executor; callers may subsequently wait
/// on the returned descriptors without creating a second execution path.
pub async fn execute_source_projection_batch(
    ctx: &ServiceContext,
    operation: ProjectionOperation,
    preflight: ProjectionPreflight<PreparedSourceItem>,
    auth: Option<AuthSnapshot>,
) -> Result<BatchResult<SourceResult>, ApiError> {
    enqueue_source_projection_batch(ctx, operation, preflight, auth).await
}

async fn admit_source_batch(
    store: Arc<dyn JobStore>,
    operation: ProjectionOperation,
    preflight: ProjectionPreflight<PreparedSourceItem>,
    auth: Option<AuthSnapshot>,
) -> Result<BatchResult<SourceResult>, ApiError> {
    let principal_id = principal_digest(auth.as_ref());
    let admission_items = preflight
        .items
        .iter()
        .map(|item| admission_item(operation, item, &principal_id, auth.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    let admitted = store
        .admit_projection_batch_atomic(ProjectionBatchAdmission {
            batch_id: preflight.batch_id,
            principal_id,
            items: admission_items,
        })
        .await?;
    let items = admitted
        .items
        .into_iter()
        .map(|item| BatchItem {
            index: item.index,
            input: Some(preflight.items[item.index].request.source.clone()),
            outcome: BatchOutcome::Queued(item.descriptor),
        })
        .collect::<Vec<_>>();
    Ok(BatchResult {
        batch_id: admitted.batch_id,
        status: BatchStatus::Accepted,
        summary: BatchSummary {
            total: items.len(),
            completed: 0,
            queued: items.len(),
            failed: 0,
            canceled: 0,
        },
        items,
    })
}

pub async fn execute_code_search_projection_batch(
    ctx: &ServiceContext,
    preflight: ProjectionPreflight<PreparedCodeSearchItem>,
    caller: CodeSearchCaller,
) -> Result<BatchResult<QueryResult>, ApiError> {
    let mut items = Vec::with_capacity(preflight.items.len());
    for prepared in preflight.items {
        let plan = prepared.plan;
        let result = crate::query::code_search(
            ctx,
            &plan.query,
            CodeSearchOptions {
                limit: plan.limit,
                offset: plan.offset,
                cwd: None,
                path_prefix: plan.path_prefix,
                ensure_fresh: false,
                caller,
            },
        )
        .await;
        let outcome = match result {
            Ok(result) => BatchOutcome::Completed(QueryResult {
                results: result.results,
            }),
            Err(error) => BatchOutcome::Failed(ApiError::new(
                "projection.code_search_failed",
                ErrorStage::Retrieving,
                error.to_string(),
            )),
        };
        items.push(BatchItem {
            index: prepared.index,
            input: Some(plan.query),
            outcome,
        });
    }
    let failed = items
        .iter()
        .filter(|item| matches!(item.outcome, BatchOutcome::Failed(_)))
        .count();
    Ok(BatchResult {
        batch_id: preflight.batch_id,
        status: if failed == 0 {
            BatchStatus::Completed
        } else {
            BatchStatus::CompletedDegraded
        },
        summary: BatchSummary {
            total: items.len(),
            completed: items.len() - failed,
            queued: 0,
            failed,
            canceled: 0,
        },
        items,
    })
}

fn admission_item(
    operation: ProjectionOperation,
    prepared: &PreparedSourceItem,
    principal_id: &str,
    auth: Option<&AuthSnapshot>,
) -> Result<ProjectionAdmissionItem, ApiError> {
    let request_json = serde_json::json!({ "source_request": prepared.request });
    let fingerprint = digest_json(&serde_json::json!({
        "version": 1,
        "operation": operation,
        "request": prepared.request,
    }))?;
    let caller_key = prepared
        .request
        .idempotency_key
        .clone()
        .unwrap_or_else(|| fingerprint.clone());
    let storage_key = digest_bytes(
        format!("projection-v1\0{operation:?}\0{principal_id}\0{caller_key}").as_bytes(),
    );
    let mut auth_snapshot = auth
        .cloned()
        .unwrap_or_else(|| AuthSnapshot::trusted_system("projection-v1"));
    auth_snapshot.token_id = None;
    auth_snapshot.display_name = None;
    Ok(ProjectionAdmissionItem {
        operation,
        storage_key,
        fingerprint: RequestFingerprintV1(fingerprint),
        request: JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Acquire,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: prepared.request.execution.priority,
            idempotency_key: None,
            stage_plan: crate::source::dispatch::source_stage_plan(
                prepared.request.embed && prepared.request.scope != Some(SourceScope::Map),
            ),
            request: Some(request_json),
            auth_snapshot,
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        },
    })
}

fn principal_digest(auth: Option<&AuthSnapshot>) -> String {
    let identity = auth
        .and_then(|snapshot| snapshot.caller_id.as_deref())
        .unwrap_or("local-system");
    digest_bytes(format!("principal-v1\0{identity}").as_bytes())
}

fn digest_json(value: &serde_json::Value) -> Result<String, ApiError> {
    serde_json::to_vec(value)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| execution_error("projection.fingerprint_failed", error.to_string()))
}

fn digest_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn execution_error(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Storage, message)
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;
