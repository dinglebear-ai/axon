use super::super::{RestRouteAuth, RestRouteInfo};

pub(crate) const AGENT_TURN_ROUTES: &[RestRouteInfo] = &[
    route("GET", "/v1/agent/turns/{id}", RestRouteAuth::Read),
    route("GET", "/v1/agent/turns/{id}/events", RestRouteAuth::Read),
    route("POST", "/v1/agent/turns/{id}/cancel", RestRouteAuth::Write),
    route("POST", "/v1/agent/turns/{id}/resume", RestRouteAuth::Write),
];

const fn route(method: &'static str, path: &'static str, auth: RestRouteAuth) -> RestRouteInfo {
    RestRouteInfo {
        method,
        path,
        auth,
        openapi: true,
    }
}
