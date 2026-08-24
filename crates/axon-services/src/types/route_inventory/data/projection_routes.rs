//! Focused source/query projection routes.

use super::super::{RestRouteAuth, RestRouteInfo};

pub(crate) const PROJECTION_ROUTES: &[RestRouteInfo] = &[
    RestRouteInfo {
        method: "POST",
        path: "/v1/scrape",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/crawl",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/embed",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ingest",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/code-search",
        auth: RestRouteAuth::Read,
        openapi: true,
    },
];
