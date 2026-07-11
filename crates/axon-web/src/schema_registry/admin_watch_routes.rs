//! REST route registry entries for the `/v1/prune/*` and `/v1/watch*` surface.
//!
//! Split out of the parent `schema_registry` module to keep it under the
//! repo's monolith line cap. Spliced back into `rest_route_registry()`'s
//! output in original position by the parent module.
//!
//! `/v1/prune/dedupe` and `/v1/prune/purge` are the replacement admin-scoped
//! homes for the removed `/v1/dedupe` and `/v1/purge` live REST write routes
//! (U2-06/U2-09) — destructive cleanup now lives exclusively under the prune
//! surface alongside `/v1/prune/plan` and `/v1/prune/exec`.

use super::{RestRouteSpec, accepted, job_admin, read, write};

pub(super) static ADMIN_WATCH_ROUTES: &[RestRouteSpec] = &[
    job_admin(
        "POST",
        "/v1/prune/dedupe",
        "dedupe",
        Some("DedupeRequest"),
        "DedupeResponse",
    ),
    job_admin(
        "POST",
        "/v1/prune/purge",
        "purge",
        Some("PurgeRequest"),
        "PurgeResult",
    ),
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
    read("GET", "/v1/watch", "watch_list", "WatchListResponse"),
    write(
        "POST",
        "/v1/watch",
        "watch_create",
        Some("WatchRequest"),
        "WatchResponse",
    ),
    accepted(
        "POST",
        "/v1/watch/{id}/run",
        "watch_run",
        None,
        "WatchRunResponse",
    ),
    // Canonical source-request-backed watch surface (issue #298 REST
    // contract, `docs/pipeline-unification/surfaces/rest-contract.md` Watch
    // Routes). Distinct from the legacy `/v1/watch` task_type/task_payload
    // routes above — see `crates/axon-jobs/src/watch_store.rs` module docs.
    // `POST /v1/watches/{id}/exec` is not yet implemented on this surface and
    // is intentionally not registered.
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
    write(
        "PATCH",
        "/v1/watches/{watch_id}",
        "watches_update",
        Some("WatchUpdateRequest"),
        "WatchResult",
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
