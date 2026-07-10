//! Read-only SourceGraph routes (issue #298 GQ). Split out of `data.rs` to
//! keep it under the monolith line cap; `rest_route_inventory` chains this
//! const after `REST_ROUTE_INVENTORY`/`WATCH_ROUTES`.
//!
//! `docs/pipeline-unification/surfaces/rest-contract.md` "Graph Routes" —
//! all read-only, no caller-provided edge writes.

use super::super::{RestRouteAuth, RestRouteInfo};

pub(crate) const GRAPH_ROUTES: &[RestRouteInfo] = &[
    RestRouteInfo {
        method: "GET",
        path: "/v1/graph/kinds",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/graph/resolve",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/graph/query",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/graph/nodes/{node_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/graph/nodes/{node_id}/edges",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/graph/edges/{edge_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/graph/sources/{source_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
];
