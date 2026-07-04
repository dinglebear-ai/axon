//! Route `SourceRequest` values through the canonical resolver/router before
//! the source orchestrator performs acquisition.

use axon_api::source::{RoutePlan, SourceKind, SourceRequest};
use axon_error::{ApiError, ErrorStage};
use axon_route::{
    AdapterRegistry, InMemoryAuthorityRegistry, RouteSecurityPolicy, SourceResolver, SourceRouter,
};

use super::classify::SourceInputKind;

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub kind: SourceInputKind,
    pub route: RoutePlan,
}

pub fn resolve_source_route(request: &SourceRequest) -> Result<RoutedSource, ApiError> {
    let registry = AdapterRegistry::target_defaults();
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let resolved = resolver.resolve(request)?;
    let route = SourceRouter::new(registry).route_with_policy(
        request,
        resolved,
        RouteSecurityPolicy::trusted_tool_execution(),
    )?;
    let kind = source_kind_to_dispatch_kind(route.source.source_kind)?;

    Ok(RoutedSource { kind, route })
}

fn source_kind_to_dispatch_kind(kind: SourceKind) -> Result<SourceInputKind, ApiError> {
    match kind {
        SourceKind::Local => Ok(SourceInputKind::Local),
        SourceKind::Git => Ok(SourceInputKind::Git),
        SourceKind::Feed => Ok(SourceInputKind::Feed),
        SourceKind::Youtube => Ok(SourceInputKind::Youtube),
        SourceKind::Reddit => Ok(SourceInputKind::Reddit),
        SourceKind::Web => Ok(SourceInputKind::Web),
        SourceKind::Session => Ok(SourceInputKind::Session),
        SourceKind::Registry => Ok(SourceInputKind::Registry),
        SourceKind::Memory | SourceKind::Upload | SourceKind::CliTool | SourceKind::McpTool => {
            Err(ApiError::new(
                "source.route.unsupported_dispatch",
                ErrorStage::Routing,
                "resolved source kind does not have a source dispatch implementation yet",
            )
            .with_context("source_kind", format!("{kind:?}")))
        }
    }
}
