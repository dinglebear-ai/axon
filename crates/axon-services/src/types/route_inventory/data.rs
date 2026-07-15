use super::RestRouteAuth;
use super::RestRouteInfo;

pub(super) const REST_ROUTE_INVENTORY: &[RestRouteInfo] = &[
    RestRouteInfo {
        method: "GET",
        path: "/healthz",
        auth: RestRouteAuth::Public,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/readyz",
        auth: RestRouteAuth::Public,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/api-docs/openapi.json",
        auth: RestRouteAuth::Public,
        openapi: false,
    },
    RestRouteInfo {
        method: "GET",
        path: "/docs",
        auth: RestRouteAuth::Public,
        openapi: false,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/capabilities",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/sources",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/sources/{source_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/resolve",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/providers",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/providers/{provider}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/collections",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/domains",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/stats",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/status",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/doctor",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/query",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/retrieve",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/map",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/artifacts",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/endpoints",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/brand",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/diff",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/screenshot",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ask",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ask/stream",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/chat",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/chat/stream",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/evaluate",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/suggest",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/sources",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/summarize",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/summarize/stream",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/search",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/research",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/research/stream",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memory",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/search",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/context",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/review",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/compact",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/memories/{memory_id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/memories/{memory_id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/link",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/supersede",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/reinforce",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/contradict",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/pin",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/archive",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/{memory_id}/compact",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/import",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/memories/export",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/jobs",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/jobs/{id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/jobs/{id}/events",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/jobs/{id}/stream",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/jobs/{id}/artifacts",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/jobs",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/jobs/{id}/cancel",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/jobs/{id}/retry",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/jobs/recover",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/jobs/cleanup",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/mobile/sessions",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/mobile/sessions/{id}",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
    RestRouteInfo {
        method: "PUT",
        path: "/v1/mobile/sessions/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/mobile/sessions/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/extract",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/extract",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/extract/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/extract/{id}/cancel",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/extract/cleanup",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/extract",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/extract/recover",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/prune/plan",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/prune/exec",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    // `/v1/dedupe` and `/v1/purge` were removed (U2-06/U2-09) and repointed
    // through the prune surface below — destructive cleanup is now
    // `axon:admin`-gated, not `axon:write`.
    RestRouteInfo {
        method: "POST",
        path: "/v1/prune/dedupe",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/prune/purge",
        auth: RestRouteAuth::Admin,
        openapi: true,
    },
    // `POST /v1/watch/{id}/run` was removed per the REST contract's
    // clean-break rule (`docs/pipeline-unification/surfaces/rest-contract.md`
    // "Removed Route Behavior") — its canonical replacement,
    // `POST /v1/watches/{watch_id}/exec`, lives in `WATCH_ROUTES` below.
];

#[path = "data/watch_routes.rs"]
mod watch_routes;
pub(super) use watch_routes::WATCH_ROUTES;

#[path = "data/graph_routes.rs"]
mod graph_routes;
pub(super) use graph_routes::GRAPH_ROUTES;
