use std::sync::Arc;

use axon_api::source::*;
use axon_api::{CodeSearchCaller, CodeSearchOptions, QueryResult};
use axon_error::{ApiError, ErrorStage};
use axon_jobs::boundary::JobStore;
use sha2::{Digest, Sha256};

use super::admission::acquire_projection_admission;
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
    admit_source_batch(ctx, store, operation, preflight, auth, false).await
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
    let store = ctx.job_store().ok_or_else(|| {
        execution_error(
            "projection.job_store_unavailable",
            "projection admission requires the unified job store",
        )
    })?;
    admit_source_batch(ctx, store, operation, preflight, auth, true).await
}

async fn admit_source_batch(
    ctx: &ServiceContext,
    store: Arc<dyn JobStore>,
    operation: ProjectionOperation,
    preflight: ProjectionPreflight<PreparedSourceItem>,
    auth: Option<AuthSnapshot>,
    honor_foreground: bool,
) -> Result<BatchResult<SourceResult>, ApiError> {
    let principal_id = principal_digest(auth.as_ref());
    let _admission = acquire_projection_admission(&principal_id, &ctx.cfg().projection_batch)?;
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
    ctx.notify_unified();
    let mut items = Vec::with_capacity(admitted.items.len());
    for item in admitted.items {
        let prepared = &preflight.items[item.index];
        let wait = honor_foreground && should_wait(&prepared.request.execution);
        let outcome = if wait {
            wait_for_source_outcome(ctx, store.as_ref(), prepared, item.descriptor).await
        } else {
            BatchOutcome::Queued(item.descriptor)
        };
        items.push(redacted_source_item(item.index, outcome));
    }
    let completed = items
        .iter()
        .filter(|item| matches!(item.outcome, BatchOutcome::Completed(_)))
        .count();
    let failed = items
        .iter()
        .filter(|item| matches!(item.outcome, BatchOutcome::Failed(_)))
        .count();
    let canceled = items
        .iter()
        .filter(|item| matches!(item.outcome, BatchOutcome::Canceled))
        .count();
    let queued = items.len() - completed - failed - canceled;
    let result = BatchResult {
        batch_id: admitted.batch_id,
        status: if queued > 0 {
            BatchStatus::Accepted
        } else if canceled == items.len() {
            BatchStatus::Canceled
        } else if failed > 0 || canceled > 0 {
            BatchStatus::CompletedDegraded
        } else {
            BatchStatus::Completed
        },
        summary: BatchSummary {
            total: items.len(),
            completed,
            queued,
            failed,
            canceled,
        },
        items,
    };
    validate_response_size(&result, ctx.cfg().projection_batch.max_response_bytes)?;
    Ok(result)
}

fn should_wait(execution: &ExecutionPolicy) -> bool {
    !execution.detached
        && matches!(
            execution.mode,
            ExecutionMode::Foreground | ExecutionMode::Wait
        )
}

fn redacted_source_item(
    index: usize,
    outcome: BatchOutcome<SourceResult>,
) -> BatchItem<SourceResult> {
    BatchItem {
        index,
        input: None,
        outcome,
    }
}

async fn wait_for_source_outcome(
    ctx: &ServiceContext,
    store: &dyn JobStore,
    prepared: &PreparedSourceItem,
    descriptor: JobDescriptor,
) -> BatchOutcome<SourceResult> {
    let timeout = prepared
        .request
        .execution
        .wait_timeout_secs
        .unwrap_or(ctx.cfg().projection_batch.max_elapsed_secs)
        .min(ctx.cfg().projection_batch.max_elapsed_secs);
    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        ctx.jobs.wait_for_job(descriptor.job_id.0, JobKind::Source),
    )
    .await;
    match waited {
        Err(_) => {
            return BatchOutcome::Failed(execution_error(
                "projection.foreground_timeout",
                format!("source job did not complete within {timeout}s"),
            ));
        }
        Ok(Err(error)) => {
            return BatchOutcome::Failed(execution_error(
                "projection.foreground_wait_failed",
                error.to_string(),
            ));
        }
        Ok(Ok(_)) => {}
    }
    let summary = match store.get(descriptor.job_id).await {
        Ok(Some(summary)) => summary,
        Ok(None) => {
            return BatchOutcome::Failed(execution_error(
                "projection.job_missing",
                "admitted source job disappeared",
            ));
        }
        Err(error) => return BatchOutcome::Failed(error),
    };
    match summary.status {
        LifecycleStatus::Canceled | LifecycleStatus::Expired => BatchOutcome::Canceled,
        LifecycleStatus::Failed | LifecycleStatus::Skipped => {
            BatchOutcome::Failed(execution_error(
                "projection.source_failed",
                summary
                    .last_error
                    .map_or_else(|| "source job failed".to_string(), |error| error.message),
            ))
        }
        status => BatchOutcome::Completed(source_result_from_summary(
            prepared, descriptor, summary, status,
        )),
    }
}

fn source_result_from_summary(
    prepared: &PreparedSourceItem,
    descriptor: JobDescriptor,
    summary: JobSummary,
    status: LifecycleStatus,
) -> SourceResult {
    let counts = summary.counts.unwrap_or(StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    });
    let source_counts = SourceCounts {
        items_total: counts.items_done,
        items_changed: counts.items_done,
        documents_total: counts.documents_done,
        chunks_total: counts.chunks_done,
        vector_points_total: counts.chunks_done,
        bytes_total: counts.bytes_done,
    };
    let source_id = summary
        .source_id
        .unwrap_or_else(|| prepared.route.source.source_id.clone());
    SourceResult {
        job_id: descriptor.job_id,
        source_id: source_id.clone(),
        canonical_uri: prepared.route.source.canonical_uri.clone(),
        source_kind: prepared.kind,
        adapter: prepared.route.adapter.clone(),
        scope: prepared.route.scope,
        status,
        ledger: LedgerSummary {
            source_id,
            generation: SourceGenerationId::new(""),
            committed_generation: None,
            status,
            counts: source_counts.clone(),
        },
        graph: GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: status == LifecycleStatus::CompletedDegraded,
        },
        counts: source_counts,
        warnings: summary.warnings,
        inline: None,
        job: Some(descriptor),
        watch: None,
        artifacts: Vec::new(),
        errors: summary.last_error.into_iter().collect(),
    }
}

fn validate_response_size<T: serde::Serialize>(value: &T, maximum: usize) -> Result<(), ApiError> {
    let actual = serde_json::to_vec(value)
        .map_err(|error| execution_error("projection.response_encoding_failed", error.to_string()))?
        .len();
    if actual <= maximum {
        return Ok(());
    }
    Err(execution_error(
        "projection.response_too_large",
        format!("response is {actual} bytes; maximum is {maximum} bytes"),
    ))
}

pub async fn execute_code_search_projection_batch(
    ctx: &ServiceContext,
    preflight: ProjectionPreflight<PreparedCodeSearchItem>,
    caller: CodeSearchCaller,
    auth: Option<&AuthSnapshot>,
) -> Result<BatchResult<QueryResult>, ApiError> {
    let principal_id = principal_digest(auth);
    let _admission = acquire_projection_admission(&principal_id, &ctx.cfg().projection_batch)?;
    let mut items = Vec::with_capacity(preflight.items.len());
    for prepared in preflight.items {
        let plan = prepared.plan;
        let result = crate::query::code_search(
            ctx,
            &plan.query,
            CodeSearchOptions {
                collection: plan.collection,
                limit: plan.limit,
                offset: plan.offset,
                cwd: plan.source.map(std::path::PathBuf::from),
                path_prefix: plan.path_prefix,
                language: plan.language,
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
            input: None,
            outcome,
        });
    }
    finish_code_search_batch(
        preflight.batch_id,
        items,
        ctx.cfg().projection_batch.max_response_bytes,
    )
}

fn finish_code_search_batch(
    batch_id: BatchId,
    items: Vec<BatchItem<QueryResult>>,
    max_response_bytes: usize,
) -> Result<BatchResult<QueryResult>, ApiError> {
    let failed = items
        .iter()
        .filter(|item| matches!(item.outcome, BatchOutcome::Failed(_)))
        .count();
    let result = BatchResult {
        batch_id,
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
    };
    validate_response_size(&result, max_response_bytes)?;
    Ok(result)
}

fn admission_item(
    operation: ProjectionOperation,
    prepared: &PreparedSourceItem,
    principal_id: &str,
    auth: Option<&AuthSnapshot>,
) -> Result<ProjectionAdmissionItem, ApiError> {
    let request_json = serde_json::json!({ "source_request": prepared.request });
    let mut semantic_request = prepared.request.clone();
    semantic_request.source = prepared.route.source.canonical_uri.clone();
    semantic_request.idempotency_key = None;
    let fingerprint = digest_json(&serde_json::json!({
        "version": 1,
        "operation": operation,
        "canonical_target": prepared.route.source.canonical_uri,
        "source_kind": prepared.kind,
        "route_scope": prepared.route.scope,
        "adapter": prepared.route.adapter,
        "validated_options": prepared.route.validated_options,
        "request": semantic_request,
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
    let identity = match auth {
        Some(snapshot) => format!(
            "authenticated\0{:?}\0{:?}\0{}\0{}",
            snapshot.auth_mode,
            snapshot.transport,
            snapshot.policy_version,
            snapshot.caller_id.as_deref().unwrap_or("anonymous-subject")
        ),
        None => local_principal_identity(),
    };
    digest_bytes(format!("principal-v2\0{identity}").as_bytes())
}

fn local_principal_identity() -> String {
    let instance = axon_core::paths::axon_home_dir()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .map_or_else(
            || "axon-home-unavailable".to_string(),
            |path| path.display().to_string(),
        );
    #[cfg(unix)]
    let uid = {
        use std::os::unix::fs::MetadataExt;
        axon_core::paths::axon_home_dir()
            .and_then(|path| std::fs::metadata(path).ok())
            .map_or_else(
                || "uid-unavailable".to_string(),
                |metadata| metadata.uid().to_string(),
            )
    };
    #[cfg(not(unix))]
    let uid = std::env::var("USERNAME").unwrap_or_else(|_| "user-unavailable".to_string());
    format!("local\0{instance}\0{uid}")
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
