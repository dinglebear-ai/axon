use super::super::{RestRouteAuth, RestRouteInfo};

pub(crate) const CODEX_ROUTES: &[RestRouteInfo] = &[
    route("GET", "/v1/codex", RestRouteAuth::Read),
    route("GET", "/v1/codex/events", RestRouteAuth::Read),
    route("POST", "/v1/codex/read", RestRouteAuth::Read),
    route("GET", "/v1/codex/{resource}", RestRouteAuth::Read),
    route("GET", "/v1/codex/operations", RestRouteAuth::Admin),
    route("POST", "/v1/codex/operations", RestRouteAuth::Admin),
    route(
        "POST",
        "/v1/codex/operations/{id}/approve",
        RestRouteAuth::Admin,
    ),
    route(
        "POST",
        "/v1/codex/operations/{id}/cancel",
        RestRouteAuth::Admin,
    ),
    route(
        "POST",
        "/v1/codex/operations/{id}/execute",
        RestRouteAuth::Admin,
    ),
    route(
        "POST",
        "/v1/codex/server-requests/{id}/respond",
        RestRouteAuth::Admin,
    ),
    route(
        "POST",
        "/v1/codex/operations/{id}/reconcile",
        RestRouteAuth::Admin,
    ),
];

const fn route(method: &'static str, path: &'static str, auth: RestRouteAuth) -> RestRouteInfo {
    RestRouteInfo {
        method,
        path,
        auth,
        openapi: true,
    }
}
