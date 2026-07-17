//! REST route registry entry for the `/v1/extract` start surface.
//!
//! Split out of the parent `schema_registry` module to keep it under the
//! repo's monolith line cap. Spliced back into `rest_route_registry()`'s
//! output in original position by the parent module.

use super::{RestRouteSpec, job_write};

pub(super) static EXTRACT_ROUTES: &[RestRouteSpec] = &[job_write(
    "POST",
    "/v1/extract",
    "extract",
    Some("ExtractRequest"),
    "JobDescriptor",
)];
