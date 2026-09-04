//! Post-publish audit annotations for graph mutation and cleanup.
//!
//! These operations persist typed events before the source job's terminal
//! transition, never as active child jobs that could outlive a terminal parent.
//! A zero-op produces no annotation; persistence failure aborts completion so
//! a terminal job can never silently omit its audit record.

use std::sync::Arc;

#[cfg(test)]
use axon_api::source::{
    AuthMode, ConfigSnapshotId, JobCreateRequest, JobIntent, JobKind, JobPriority, JobStagePlan,
    MetadataMap, TransportKind, Visibility,
};
use axon_api::source::{AuthSnapshot, GraphWriteSummary, JobId, LifecycleStatus, PipelinePhase};
use axon_jobs::boundary::JobStore;
#[cfg(test)]
use axon_jobs::workers::auth_enforcement::child_auth_snapshot;

use super::prune::DebtDrainSummary;

/// Auth snapshot for a child job when no real parent snapshot is available
/// (degraded/system-triggered paths that never reach this call site with a
/// real caller identity in practice, but the fallback must still fail
/// closed). Carries **no** elevated scopes -- mirrors the
/// the source-watch scheduler's no-elevation rule -- instead of defaulting to
/// `AuthSnapshot::trusted_system`'s Read+Write+Admin grant, which would let
/// an unauthenticated/no-scope caller's child job silently gain admin.
#[cfg(test)]
fn no_scope_child_auth_snapshot() -> AuthSnapshot {
    AuthSnapshot {
        caller_id: None,
        transport: TransportKind::System,
        granted_scopes: Vec::new(),
        visibility_ceiling: Visibility::Internal,
        request_time: axon_api::source::Timestamp::from(chrono::Utc::now()),
        policy_version: "runtime".to_string(),
        auth_mode: AuthMode::TrustedLocal,
        token_id: None,
        display_name: None,
    }
}

/// Build the child-job create request shared by graph/prune tracking.
///
/// `parent_auth_snapshot` is the real caller's snapshot threaded in from
/// `index_source_with_auth`. When present, the child job inherits exactly
/// the parent's grants via `child_auth_snapshot` -- never more. When absent
/// (no real caller identity, e.g. a system-triggered run), the child job
/// gets [`no_scope_child_auth_snapshot`] rather than an elevated default.
#[cfg(test)]
fn child_job_request(
    parent_job_id: JobId,
    parent_auth_snapshot: Option<&AuthSnapshot>,
    job_kind: JobKind,
    job_intent: JobIntent,
    phase: PipelinePhase,
    result_schema: &str,
) -> JobCreateRequest {
    let auth_snapshot = match parent_auth_snapshot {
        Some(parent) => child_auth_snapshot(parent),
        None => no_scope_child_auth_snapshot(),
    };
    JobCreateRequest {
        request_id: None,
        job_kind,
        job_intent,
        source_id: None,
        watch_id: None,
        parent_job_id: Some(parent_job_id),
        root_job_id: Some(parent_job_id),
        attempt: 1,
        priority: JobPriority::Background,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan::required(phase)],
        request: None,
        auth_snapshot,
        config_snapshot_id: Some(ConfigSnapshotId::new("runtime")),
        requirements: MetadataMap::new(),
        result_schema: Some(result_schema.to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Record the baseline graph write as a terminal annotation on `parent_job_id`.
///
/// Skips annotation (returns immediately) when there is no unified job
/// store, the parent job id is the nil placeholder (degraded/no-data-plane
/// paths never reach this call site in practice, but the guard is defensive),
/// or the graph write was a true no-op (`degraded` with zero counts — nothing
/// happened worth a job row).
pub async fn track_graph_mutation(
    job_store: Option<Arc<dyn JobStore>>,
    parent_job_id: JobId,
    _parent_auth_snapshot: Option<&AuthSnapshot>,
    summary: &GraphWriteSummary,
) -> Result<(), axon_api::source::ApiError> {
    let Some(store) = job_store else {
        return Ok(());
    };
    if parent_job_id.0.is_nil() {
        return Ok(());
    }
    if summary.nodes_upserted == 0 && summary.edges_upserted == 0 && summary.evidence_records == 0 {
        return Ok(());
    }

    let message = format!(
        "baseline graph write: nodes={} edges={} evidence={} degraded={}",
        summary.nodes_upserted, summary.edges_upserted, summary.evidence_records, summary.degraded
    );
    let mut event = axon_api::source::SourceProgressEvent::minimal(
        parent_job_id,
        0,
        PipelinePhase::Graphing,
        if summary.degraded {
            LifecycleStatus::Failed
        } else {
            LifecycleStatus::Completed
        },
        if summary.degraded {
            axon_api::source::Severity::Warning
        } else {
            axon_api::source::Severity::Info
        },
        message,
    );
    event.dedupe_key = Some("post-publish:graph".into());
    store.append_event(event).await
}

/// Record the cleanup-debt drain as a terminal annotation on `parent_job_id`.
///
/// Skips annotation when there is no unified job store, the parent job id
/// is the nil placeholder, or the drain found no pending debt (the common
/// case for most source indexes — nothing to prune yet).
pub async fn track_prune(
    job_store: Option<Arc<dyn JobStore>>,
    parent_job_id: JobId,
    _parent_auth_snapshot: Option<&AuthSnapshot>,
    summary: &DebtDrainSummary,
) -> Result<(), axon_api::source::ApiError> {
    let Some(store) = job_store else {
        return Ok(());
    };
    if parent_job_id.0.is_nil() {
        return Ok(());
    }
    if summary.resolved == 0 && summary.failed == 0 {
        return Ok(());
    }

    let message = format!(
        "cleanup debt drain: resolved={} failed={} points_deleted={}",
        summary.resolved, summary.failed, summary.points_deleted
    );
    // `drain_cleanup_debt` never returns an error by contract; the drain is
    // considered fully successful only when nothing was left pending.
    let mut event = axon_api::source::SourceProgressEvent::minimal(
        parent_job_id,
        0,
        PipelinePhase::Cleaning,
        if summary.failed == 0 {
            LifecycleStatus::Completed
        } else {
            LifecycleStatus::Failed
        },
        if summary.failed == 0 {
            axon_api::source::Severity::Info
        } else {
            axon_api::source::Severity::Warning
        },
        message,
    );
    event.dedupe_key = Some("post-publish:cleanup".into());
    store.append_event(event).await
}

#[cfg(test)]
#[path = "job_tracking_tests.rs"]
mod tests;
