//! Detached-enqueue path for a [`SourceRequest`].
//!
//! [`enqueue_source`] is the counterpart to
//! [`crate::source::index_source_with_auth`] for callers that want a
//! `SourceRequest` to run as a detached `JobKind::Source` row instead of
//! blocking inline. It performs the same pre-dispatch validation and routing
//! as `index_source_with_auth` (empty-input check, route resolution,
//! authorize-stage check), then — instead of dispatching to a family bridge —
//! creates a `JobKind::Source` job row carrying
//! `{"source_request": <SourceRequest JSON>}` via the unified job store. That
//! row is picked up and actually run by
//! `crate::runtime::job_runners::source_runner::SourceRunner`, which is
//! registered against `JobKind::Source` and calls
//! `index_source_with_auth` for real (see `job-contract.md` and bead
//! `axon_rust-mijoc`). Before this module, no transport enqueued a detached
//! `Source` row, so that runner was unreachable in production.
//!
//! Idempotency: `JobCreateRequest.idempotency_key` (threaded straight from
//! `SourceRequest.idempotency_key`) is honored by the unified store's own
//! `create()` — a matching existing job row (any status, not just in-flight)
//! is returned instead of a duplicate insert. See
//! `SqliteUnifiedJobStore::create_job` / `find_by_idempotency_key`.

use axon_api::source::{
    AdapterRef, AuthScope, AuthSnapshot, JobCreateRequest, JobIntent, JobKind, MetadataMap,
    SourceIntent, SourceKind, SourceRequest, SourceResult, SourceScope,
};
use axon_error::{ApiError, ErrorStage};
use axon_jobs::boundary::JobStore;
use std::path::PathBuf;

use super::authorize;
use super::result_map;
use super::routing;

/// Validate + route `request`, then enqueue a detached `JobKind::Source` job
/// instead of running acquisition inline.
///
/// Returns a `SourceResult` carrying `job = Some(descriptor)` and
/// `status` set to whatever the store returned for that job (normally
/// `Queued`). Routing/authorization failures degrade to a `Failed`
/// `SourceResult` exactly like `index_source_with_auth`, never an `Err`; only
/// job-store errors bubble up as `Err`.
pub async fn enqueue_source(
    request: SourceRequest,
    store: &dyn JobStore,
    auth_snapshot: Option<AuthSnapshot>,
) -> anyhow::Result<SourceResult> {
    enqueue_source_with_allowed_roots(request, store, auth_snapshot, None).await
}

/// Config-aware detached enqueue used by server transports. Local requests
/// are rejected before a durable job is created unless they resolve beneath a
/// configured allowed root. Trusted-local CLI/system snapshots intentionally
/// retain their direct-filesystem behavior.
pub async fn enqueue_source_with_allowed_roots(
    request: SourceRequest,
    store: &dyn JobStore,
    auth_snapshot: Option<AuthSnapshot>,
    allowed_roots: Option<&[PathBuf]>,
) -> anyhow::Result<SourceResult> {
    let input = request.source.trim().to_string();
    if input.is_empty() {
        return Ok(result_map::unsupported_result(
            &request.source,
            "source request requires a non-empty local path, git URL, feed URL, youtube target, \
             reddit target, web URL, session selector, or registry target",
        ));
    }

    let routed = match routing::resolve_source_route_for_auth(&request, auth_snapshot.as_ref()) {
        Ok(routed) => routed,
        Err(err) => return Ok(result_map::route_error_result(&input, err)),
    };
    if let Err(err) = authorize::authorize_route(&routed.route) {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Err(err) =
        authorize::authorize_safety_class(routed.route.safety_class, auth_snapshot.as_ref())
    {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Err(err) = authorize::authorize_execution_affinity(
        routed.route.safety_class,
        axon_api::source::ExecutionAffinity::Scheduler,
        auth_snapshot.as_ref(),
    ) {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Err(err) =
        authorize_detached_local_source_policy(&input, routed.kind, auth_snapshot.as_ref())
    {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Some(allowed_roots) = allowed_roots
        && let Err(err) = super::security::authorize_local_source_allowed_roots(
            &input,
            routed.kind,
            auth_snapshot.as_ref(),
            allowed_roots,
        )
    {
        return Ok(result_map::route_error_result(&input, err));
    }
    let auth_snapshot = auth_snapshot.unwrap_or_else(|| AuthSnapshot::trusted_system("runtime"));
    let adapter = routed.route.adapter.clone();
    let source_kind = routed.route.source.source_kind;
    let scope = routed.route.scope;
    let canonical_uri = routed.route.source.canonical_uri.clone();

    if routed.kind == SourceKind::CliTool {
        axon_adapters::cli_tool::validate_persistable_cli_invocation(&request.source)
            .map_err(|error| ApiError::new(error.code, ErrorStage::Authorizing, error.message))?;
    }

    let descriptor = store
        .create(job_create_request(&request, auth_snapshot))
        .await?;

    Ok(result_map::queued_result(
        source_kind,
        adapter,
        scope,
        canonical_uri,
        descriptor,
    ))
}

fn authorize_detached_local_source_policy(
    input: &str,
    kind: SourceKind,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    if kind != SourceKind::Local {
        return Ok(());
    }
    let has_local_scope = auth_snapshot
        .map(|snapshot| authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(false);
    super::enforce_local_source_policy(input, has_local_scope)
        .map_err(|err| ApiError::new(err.code, ErrorStage::Authorizing, err.message))
}

/// Build the `JobKind::Source` create request. `claimed.request_json` on the
/// consuming side (`SourceRunner`) expects `{"source_request": <..>}`.
fn job_create_request(request: &SourceRequest, auth_snapshot: AuthSnapshot) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: source_intent_to_job_intent(request.intent),
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: request.execution.priority,
        idempotency_key: request.idempotency_key.clone(),
        stage_plan: super::dispatch::source_stage_plan(
            request.embed && request.scope != Some(SourceScope::Map),
        ),
        request: Some(serde_json::json!({ "source_request": request })),
        auth_snapshot,
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Map the caller-facing `SourceRequest.intent` onto the durable job's
/// `job_intent` (R1-08) instead of the catch-all `JobIntent::Run`.
fn source_intent_to_job_intent(intent: SourceIntent) -> JobIntent {
    match intent {
        SourceIntent::Acquire => JobIntent::Acquire,
        SourceIntent::Refresh => JobIntent::Refresh,
        SourceIntent::Watch => JobIntent::Watch,
        SourceIntent::Map => JobIntent::Map,
    }
}

#[cfg(test)]
#[path = "enqueue_tests.rs"]
mod tests;
