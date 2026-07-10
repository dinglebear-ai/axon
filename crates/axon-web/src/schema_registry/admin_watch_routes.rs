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
];
