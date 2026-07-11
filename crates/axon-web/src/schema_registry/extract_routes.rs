//! REST route registry entries for the `/v1/extract` job-family surface.
//!
//! Split out of the parent `schema_registry` module to keep it under the
//! repo's monolith line cap. Spliced back into `rest_route_registry()`'s
//! output in original position by the parent module.

use super::{RestRouteSpec, job_read, job_write};

pub(super) static EXTRACT_ROUTES: &[RestRouteSpec] = &[
    job_read("GET", "/v1/extract", "extract_list", "JobListResponse"),
    job_write(
        "POST",
        "/v1/extract",
        "extract",
        Some("ExtractRequest"),
        "JobDescriptor",
    ),
    job_write(
        "DELETE",
        "/v1/extract",
        "extract_clear",
        None,
        "JobCleanupResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/cleanup",
        "extract_cleanup",
        None,
        "JobCleanupResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/recover",
        "extract_recover",
        None,
        "JobRecoveryResponse",
    ),
    job_read(
        "GET",
        "/v1/extract/{id}",
        "extract_status",
        "JobStatusResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/{id}/cancel",
        "extract_cancel",
        None,
        "JobCancelResponse",
    ),
];
