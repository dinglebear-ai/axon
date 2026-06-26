#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestRouteAuth {
    Public,
    /// Protected metadata/retrieval bucket. Runtime OAuth accepts any Axon
    /// scope (`axon:read` or `axon:write`) for protected Axon REST routes.
    Read,
    /// Protected active-operation bucket. Runtime OAuth accepts any Axon
    /// scope (`axon:read` or `axon:write`) for protected Axon REST routes.
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestRouteInfo {
    pub method: &'static str,
    pub path: &'static str,
    pub auth: RestRouteAuth,
    /// True when the route is intentionally represented in the generated
    /// OpenAPI document. Runtime-only docs assets such as Swagger UI are kept
    /// public but are not OpenAPI operations themselves.
    pub openapi: bool,
}

impl RestRouteInfo {
    pub const fn route(self) -> &'static str {
        self.path
    }

    pub fn display(self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

pub fn rest_route_inventory() -> &'static [RestRouteInfo] {
    REST_ROUTE_INVENTORY
}

pub fn supported_routes() -> Vec<String> {
    REST_ROUTE_INVENTORY
        .iter()
        .map(|route| route.display())
        .collect()
}

const REST_ROUTE_INVENTORY: &[RestRouteInfo] = &[
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
        path: "/v1/scrape",
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
        path: "/v1/crawl",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/crawl",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/crawl/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/crawl/{id}/cancel",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/crawl/cleanup",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/crawl",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/crawl/recover",
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
        method: "GET",
        path: "/v1/embed",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/embed/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/embed/{id}/cancel",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/embed/cleanup",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/embed",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/embed/recover",
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
        path: "/v1/ingest",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ingest/sessions/prepared",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/ingest",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/ingest/{id}",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ingest/{id}/cancel",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ingest/cleanup",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "DELETE",
        path: "/v1/ingest",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/ingest/recover",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/dedupe",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/purge",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "GET",
        path: "/v1/watch",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/watch",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
    RestRouteInfo {
        method: "POST",
        path: "/v1/watch/{id}/run",
        auth: RestRouteAuth::Write,
        openapi: true,
    },
];
