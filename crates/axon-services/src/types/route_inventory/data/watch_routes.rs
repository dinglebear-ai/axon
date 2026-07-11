//! Canonical source-request-backed watch routes (issue #298 WS-B). Split out
//! of `data.rs` to keep it under the monolith line cap; `rest_route_inventory`
//! chains this const after `REST_ROUTE_INVENTORY`.

use super::super::{RestRouteAuth, RestRouteInfo};

pub(crate) const WATCH_ROUTES: &[RestRouteInfo] = &[
    // Canonical source-request-backed watch surface (issue #298 WS-B REST
    // contract, `docs/pipeline-unification/surfaces/rest-contract.md` Watch
    // Routes). Distinct from the legacy `/v1/watch` routes above.
    RestRouteInfo {
        method: "POST",
        path: "/v1/watches",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/watches",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/watches/{watch_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "PATCH",
        path: "/v1/watches/{watch_id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/watches/{watch_id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/watches/{watch_id}/pause",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/watches/{watch_id}/resume",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
];
