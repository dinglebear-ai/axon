#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestRouteAuth {
    Public,
    /// Protected metadata/retrieval bucket. Runtime OAuth accepts any Axon
    /// scope (`axon:read` or `axon:write`) for protected Axon REST routes.
    Read,
    /// Protected active-operation bucket. Runtime OAuth accepts any Axon
    /// scope (`axon:read` or `axon:write`) for protected Axon REST routes.
    Write,
    /// Protected administrative/destructive bucket. Runtime requires explicit
    /// `axon:admin`; broad write tokens are insufficient.
    Admin,
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

mod data;
use data::REST_ROUTE_INVENTORY;
