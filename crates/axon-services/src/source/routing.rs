//! Route `SourceRequest` values through the canonical resolver/router before
//! the source orchestrator performs acquisition.

use axon_api::source::{
    AdapterRef, AuthSnapshot, PipelinePhase, RoutePlan, SourceKind, SourceRequest,
};
use axon_error::{ApiError, ErrorStage};
use axon_route::{
    AdapterRegistry, InMemoryAuthorityRegistry, RouteSecurityPolicy, SourceResolver, SourceRouter,
};
use std::sync::OnceLock;

use super::classify::SourceInputKind;
use super::events::SourceEventEmitter;
use super::security::authorize_local_source_policy;

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub kind: SourceInputKind,
    pub route: RoutePlan,
}

pub(crate) struct AuthorizedSourceRoute {
    pub kind: SourceInputKind,
    pub route: RoutePlan,
    pub adapter: AdapterRef,
    pub(crate) event_emitter: SourceEventEmitter,
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

pub(crate) async fn resolve_authorized_source_route(
    request: &SourceRequest,
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    event_emitter: SourceEventEmitter,
) -> Result<AuthorizedSourceRoute, ApiError> {
    event_emitter
        .running(PipelinePhase::Resolving, "resolving source request")
        .await;
    let routed = match resolve_source_route(request) {
        Ok(routed) => routed,
        Err(err) => {
            event_emitter
                .failed(
                    PipelinePhase::Resolving,
                    "source request route resolution failed",
                )
                .await;
            return Err(err);
        }
    };
    let kind = routed.kind;
    let route = routed.route;
    let adapter = AdapterRef {
        name: route.adapter.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let event_emitter =
        event_emitter.with_route(route.source.source_kind, route.scope, adapter.clone());
    event_emitter
        .running(PipelinePhase::Routing, "routing source request")
        .await;
    event_emitter
        .running(PipelinePhase::Authorizing, "authorizing source request")
        .await;
    authorize_route_plan(&route, input, kind, auth_snapshot, &event_emitter).await?;
    Ok(AuthorizedSourceRoute {
        kind,
        route,
        adapter,
        event_emitter,
    })
}

async fn authorize_route_plan(
    route: &RoutePlan,
    input: &str,
    kind: SourceInputKind,
    auth_snapshot: Option<&AuthSnapshot>,
    event_emitter: &SourceEventEmitter,
) -> Result<(), ApiError> {
    if let Err(err) = super::authorize::authorize_route(route) {
        event_emitter
            .failed(
                PipelinePhase::Authorizing,
                "source route authorization failed",
            )
            .await;
        return Err(err);
    }
    if let Err(err) = super::authorize::authorize_safety_class(route.safety_class, auth_snapshot) {
        event_emitter
            .failed(
                PipelinePhase::Authorizing,
                "source safety authorization failed",
            )
            .await;
        return Err(err);
    }
    if let Err(err) = authorize_local_source_policy(input, kind, auth_snapshot) {
        event_emitter
            .failed(
                PipelinePhase::Authorizing,
                "local source authorization failed",
            )
            .await;
        return Err(err);
    }
    if kind == SourceInputKind::Unsupported {
        event_emitter
            .failed(PipelinePhase::Routing, "source kind is unsupported")
            .await;
        return Err(ApiError::new(
            "source.route.unsupported_dispatch",
            ErrorStage::Routing,
            "resolved source kind does not have a source dispatch implementation yet",
        )
        .with_context("source_kind", format!("{:?}", route.source.source_kind)));
    }
    Ok(())
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
        SourceKind::CliTool => SourceInputKind::CliTool,
        SourceKind::McpTool => SourceInputKind::McpTool,
        SourceKind::Memory => SourceInputKind::Memory,
        SourceKind::Upload => SourceInputKind::Upload,
    }
}
