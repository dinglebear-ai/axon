//! REST route registry entries for the `/v1/prune/*` and canonical `/v1/watches*` surface.
//!
//! Split out of the parent `schema_registry` module to keep it under the
//! repo's monolith line cap. Spliced back into `rest_route_registry()`'s
//! output in original position by the parent module.
//!
//! Cleanup is plan-first: only `/v1/prune/plan` and `/v1/prune/exec` are public.

use super::{RestRouteSpec, job_admin, read, write};

pub(super) static ADMIN_WATCH_ROUTES: &[RestRouteSpec] = &[
    job_admin(
        "POST",
        "/v1/prune/plan",
        "prune_plan",
        Some("PrunePlanRequest"),
        "PrunePlan",
    ),
    job_admin(
        "POST",
        "/v1/prune/exec",
        "prune_exec",
        Some("PruneExecRequest"),
        "PruneResult",
    ),
    job_admin(
        "POST",
        "/v1/reset/plan",
        "plan_reset",
        Some("ResetPlanRequest"),
        "ResetPlan",
    ),
    job_admin(
        "POST",
        "/v1/reset/exec",
        "execute_reset",
        Some("ResetExecRequest"),
        "ResetResult",
    ),
    // `POST /v1/watch/{id}/run` was removed per the REST contract's
    // clean-break rule (`docs/pipeline-unification/surfaces/rest-contract.md`
    // "Removed Route Behavior") — its canonical replacement is
    // `POST /v1/watches/{watch_id}/exec` below.
    //
    // Canonical source-request-backed watch surface (issue #298 REST
    // contract, `docs/pipeline-unification/surfaces/rest-contract.md` Watch
    // Routes).
    write(
        "POST",
        "/v1/watches",
        "watches_create",
        Some("WatchRequest"),
        "WatchResult",
    ),
    read("GET", "/v1/watches", "watches_list", "Page<WatchSummary>"),
    read(
        "GET",
        "/v1/watches/{watch_id}",
        "watches_get",
        "WatchResult",
    ),
    read(
        "GET",
        "/v1/watches/{watch_id}/status",
        "watches_status",
        "WatchStatusResult",
    ),
    read(
        "GET",
        "/v1/watches/{watch_id}/history",
        "watches_history",
        "WatchHistoryResult",
    ),
    write(
        "PATCH",
        "/v1/watches/{watch_id}",
        "watches_update",
        Some("WatchUpdateRequest"),
        "WatchResult",
    ),
    write(
        "POST",
        "/v1/watches/{watch_id}/exec",
        "watches_exec",
        Some("WatchExecRequest"),
        "JobDescriptor",
    ),
    write(
        "DELETE",
        "/v1/watches/{watch_id}",
        "watches_delete",
        None,
        "WatchDeleteResponse",
    ),
    write(
        "POST",
        "/v1/watches/{watch_id}/pause",
        "watches_pause",
        None,
        "WatchResult",
    ),
    write(
        "POST",
        "/v1/watches/{watch_id}/resume",
        "watches_resume",
        None,
        "WatchResult",
    ),
];
