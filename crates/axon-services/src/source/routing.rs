//! Route `SourceRequest` values through the canonical resolver/router before
//! the source orchestrator performs acquisition.

use axon_api::source::{RoutePlan, SourceKind, SourceRequest};
use axon_error::ApiError;
use axon_route::{
    AdapterRegistry, InMemoryAuthorityRegistry, RouteSecurityPolicy, SourceResolver, SourceRouter,
};
use std::sync::OnceLock;

use super::classify::SourceInputKind;

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub kind: SourceInputKind,
    pub route: RoutePlan,
}

pub fn resolve_source_route(request: &SourceRequest) -> Result<RoutedSource, ApiError> {
    let components = route_components();
    let resolved = components.resolver.resolve(request)?;
    let route = components.router.route_with_policy(
        request,
        resolved,
        RouteSecurityPolicy::trusted_tool_execution(),
    )?;
    let kind = source_kind_to_dispatch_kind(route.source.source_kind);

    Ok(RoutedSource { kind, route })
}

struct RouteComponents {
    resolver: SourceResolver,
    router: SourceRouter,
}

fn route_components() -> &'static RouteComponents {
    static COMPONENTS: OnceLock<RouteComponents> = OnceLock::new();
    COMPONENTS.get_or_init(|| {
        let registry = AdapterRegistry::target_defaults();
        let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
        let router = SourceRouter::new(registry);
        RouteComponents { resolver, router }
    })
}

fn source_kind_to_dispatch_kind(kind: SourceKind) -> SourceInputKind {
    match kind {
        SourceKind::Local => SourceInputKind::Local,
        SourceKind::Git => SourceInputKind::Git,
        SourceKind::Feed => SourceInputKind::Feed,
        SourceKind::Youtube => SourceInputKind::Youtube,
        SourceKind::Reddit => SourceInputKind::Reddit,
        SourceKind::Web => SourceInputKind::Web,
        SourceKind::Session => SourceInputKind::Session,
        SourceKind::Registry => SourceInputKind::Registry,
        SourceKind::Memory | SourceKind::Upload | SourceKind::CliTool | SourceKind::McpTool => {
            SourceInputKind::Unsupported
        }
    }
}
