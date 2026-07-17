//! Child job tracking for `index_source`'s graph-mutation and prune sub-steps.
//!
//! [`write_baseline_graph`](super::graph::write_baseline_graph) and
//! [`drain_cleanup_debt`](super::prune::drain_cleanup_debt) run as sub-steps of
//! one `index_source` call, not as standalone top-level operations — there is
//! no `axon graph` or `axon prune` CLI/MCP surface yet (see
//! `docs/pipeline-unification/surfaces/command-contract.md`, which specifies
//! `prune` as a *future* top-level command still owned by the PR0-skeleton
//! `axon-prune` crate). Per `docs/pipeline-unification/runtime/job-contract.md`,
//! `graph` and `prune` are still real `JobKind`s in the target model, and
//! "foreground CLI operations still create a job row when they perform ...
//! graph mutation, pruning, ...". This module satisfies that by recording each
//! non-trivial sub-step as a **child job** of the parent `Source` job
//! (`parent_job_id`/`root_job_id`), rather than inventing a new standalone
//! command surface.
//!
//! Child jobs are only created when there is a real unified job store *and* a
//! real parent job id (the nil UUID placeholder used by degraded/no-data-plane
//! paths is skipped) *and* the sub-step actually did something — a zero-op
//! graph write or an empty cleanup-debt drain does not spam a job row for
//! every tiny source index. Job-store errors during tracking are logged and
//! swallowed: the parent source index is already committed, so a tracking
//! failure must never fail acquisition.
//!
//! ## The parent `Source` job is already terminal when these children attach
//!
//! Every source pipeline execution
//! records its own `Source` job's terminal status (`Completed`/`Failed`)
//! *before* [`index_source_with_auth`](super::index_source_with_auth) reaches
//! the graph write and cleanup-debt drain — acquisition, embedding, and
//! publish are the source's own contract, and that contract is fully
//! committed by the time this module runs. `write_baseline_graph` and
//! `drain_cleanup_debt` are best-effort *post*-publish housekeeping over the
//! manifest the source just committed, not additional required stages of the
//! source's own completion.
//!
//! This means the `parent_job_id`/`root_job_id` these children carry point at
//! a `Source` job that is already in a terminal state by the time the child
//! is created. That is intentional here, not a bug: it inverts the usual
//! fan-out/fan-in expectation from `docs/pipeline-unification/runtime/
//! job-contract.md` ("Parent jobs aggregate child status" — i.e. a parent
//! normally waits on its children) because these children are not gating
//! whether the source publish succeeded; they are an audit trail for work
//! that runs *after* publish is already durable. The job store does not
//! (and must not) reject child-job creation or transitions against a
//! terminal parent — see
//! `child_jobs_attach_to_an_already_terminal_parent_source_job` in the
//! sidecar tests for a regression pin of this exact sequence.
//!
//! Reordering acquisition to defer the parent's terminal status until after
//! graph/prune would require threading a "don't finalize yet" signal through
//! the source pipelines' internal progress sinks (each one calls its
//! own multi-phase `record_phase`/equivalent deep inside its own pipeline,
//! independently of `index_source_with_auth`), and would delay job-visible
//! completion of the source index for what is otherwise pure post-publish
//! bookkeeping. That tradeoff was rejected in favor of documenting and
//! testing the current, narrower ordering.

use std::sync::Arc;

use axon_api::source::{
    AuthMode, AuthSnapshot, ConfigSnapshotId, GraphWriteSummary, JobCreateRequest, JobId,
    JobIntent, JobKind, JobPriority, JobStagePlan, JobStatusUpdate, LifecycleStatus, MetadataMap,
    PipelinePhase, TransportKind, Visibility,
};
use axon_jobs::boundary::JobStore;
use axon_jobs::workers::auth_enforcement::child_auth_snapshot;

use super::prune::DebtDrainSummary;

/// Auth snapshot for a child job when no real parent snapshot is available
/// (degraded/system-triggered paths that never reach this call site with a
/// real caller identity in practice, but the fallback must still fail
/// closed). Carries **no** elevated scopes -- mirrors the
/// the source-watch scheduler's no-elevation rule -- instead of defaulting to
/// `AuthSnapshot::trusted_system`'s Read+Write+Admin grant, which would let
/// an unauthenticated/no-scope caller's child job silently gain admin.
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
        stage_plan: vec![JobStagePlan {
            phase,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: None,
        }],
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

/// Transition a freshly-created child job `Queued -> Running -> {Completed,Failed}`.
/// Every step is best-effort: a job-store error here is logged and ignored, it
/// never propagates to the caller.
async fn run_tracked(
    store: &Arc<dyn JobStore>,
    job_id: JobId,
    phase: PipelinePhase,
    ok: bool,
    message: String,
) {
    let running = JobStatusUpdate {
        job_id,
        source_id: None,
        status: LifecycleStatus::Running,
        phase,
        stage_id: None,
        counts: None,
        current: None,
        message: None,
        error: None,
    };
    if let Err(err) = store.update_status(running).await {
        tracing::warn!(
            job_id = %job_id.0,
            error = %err.message,
            "failed to transition child job to running; leaving as queued"
        );
        return;
    }

    let terminal_status = if ok {
        LifecycleStatus::Completed
    } else {
        LifecycleStatus::Failed
    };
    let terminal = JobStatusUpdate {
        job_id,
        source_id: None,
        status: terminal_status,
        phase,
        stage_id: None,
        counts: None,
        current: None,
        message: Some(message),
        error: None,
    };
    if let Err(err) = store.update_status(terminal).await {
        tracing::warn!(
            job_id = %job_id.0,
            error = %err.message,
            "failed to transition child job to terminal status"
        );
    }
}

/// Record the baseline graph write as a child `graph` job of `parent_job_id`,
/// when the write actually produced or attempted non-trivial output.
///
/// Skips job creation (returns immediately) when there is no unified job
/// store, the parent job id is the nil placeholder (degraded/no-data-plane
/// paths never reach this call site in practice, but the guard is defensive),
/// or the graph write was a true no-op (`degraded` with zero counts — nothing
/// happened worth a job row).
pub async fn track_graph_mutation(
    job_store: Option<Arc<dyn JobStore>>,
    parent_job_id: JobId,
    parent_auth_snapshot: Option<&AuthSnapshot>,
    summary: &GraphWriteSummary,
) {
    let Some(store) = job_store else {
        return;
    };
    if parent_job_id.0.is_nil() {
        return;
    }
    if summary.nodes_upserted == 0 && summary.edges_upserted == 0 && summary.evidence_records == 0 {
        return;
    }

    let request = child_job_request(
        parent_job_id,
        parent_auth_snapshot,
        JobKind::Graph,
        JobIntent::Run,
        PipelinePhase::Graphing,
        "graph_result",
    );
    let descriptor = match store.create(request).await {
        Ok(descriptor) => descriptor,
        Err(err) => {
            tracing::warn!(
                parent_job_id = %parent_job_id.0,
                error = %err.message,
                "failed to create child graph-mutation job; skipping tracking"
            );
            return;
        }
    };

    let message = format!(
        "baseline graph write: nodes={} edges={} evidence={} degraded={}",
        summary.nodes_upserted, summary.edges_upserted, summary.evidence_records, summary.degraded
    );
    run_tracked(
        &store,
        descriptor.job_id,
        PipelinePhase::Graphing,
        !summary.degraded,
        message,
    )
    .await;
}

/// Record the cleanup-debt drain as a child `prune` job of `parent_job_id`,
/// when the drain actually touched at least one debt entry.
///
/// Skips job creation when there is no unified job store, the parent job id
/// is the nil placeholder, or the drain found no pending debt (the common
/// case for most source indexes — nothing to prune yet).
pub async fn track_prune(
    job_store: Option<Arc<dyn JobStore>>,
    parent_job_id: JobId,
    parent_auth_snapshot: Option<&AuthSnapshot>,
    summary: &DebtDrainSummary,
) {
    let Some(store) = job_store else {
        return;
    };
    if parent_job_id.0.is_nil() {
        return;
    }
    if summary.resolved == 0 && summary.failed == 0 {
        return;
    }

    let request = child_job_request(
        parent_job_id,
        parent_auth_snapshot,
        JobKind::Prune,
        JobIntent::Cleanup,
        PipelinePhase::Cleaning,
        "prune_result",
    );
    let descriptor = match store.create(request).await {
        Ok(descriptor) => descriptor,
        Err(err) => {
            tracing::warn!(
                parent_job_id = %parent_job_id.0,
                error = %err.message,
                "failed to create child prune job; skipping tracking"
            );
            return;
        }
    };

    let message = format!(
        "cleanup debt drain: resolved={} failed={} points_deleted={}",
        summary.resolved, summary.failed, summary.points_deleted
    );
    // `drain_cleanup_debt` never returns an error by contract; the drain is
    // considered fully successful only when nothing was left pending.
    run_tracked(
        &store,
        descriptor.job_id,
        PipelinePhase::Cleaning,
        summary.failed == 0,
        message,
    )
    .await;
}

#[cfg(test)]
#[path = "job_tracking_tests.rs"]
mod tests;
