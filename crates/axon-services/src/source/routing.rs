//! Route `SourceRequest` values through the canonical resolver/router before
//! the source orchestrator performs acquisition.

use axon_api::source::{
    AdapterRef, AuthSnapshot, ExecutionAffinity, PipelinePhase, RoutePlan, SourceKind,
    SourceRequest,
};
use axon_error::{ApiError, ErrorStage};
use axon_route::{
    AdapterRegistry, InMemoryAuthorityRegistry, RouteSecurityPolicy, SourceResolver, SourceRouter,
};
use std::sync::OnceLock;

use super::authorize::SourceAccessDecision;
use super::events::SourceEventEmitter;

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub kind: SourceKind,
    pub route: RoutePlan,
}

pub(crate) struct AuthorizedSourceRoute {
    pub kind: SourceKind,
    pub route: RoutePlan,
    pub adapter: AdapterRef,
    pub(crate) event_emitter: SourceEventEmitter,
}

pub fn resolve_source_route(request: &SourceRequest) -> Result<RoutedSource, ApiError> {
    resolve_source_route_with_policy(request, RouteSecurityPolicy::default())
}

pub(crate) fn resolve_source_route_for_access(
    request: &SourceRequest,
    auth_snapshot: Option<&AuthSnapshot>,
    operator_allows_tool_execution: bool,
) -> Result<RoutedSource, ApiError> {
    let caller_allows_tool_execution = auth_snapshot.is_some_and(|snapshot| {
        super::authorize::snapshot_allows_scope(snapshot, axon_api::source::AuthScope::Execute)
    });
    let policy = RouteSecurityPolicy::from_tool_execution_authority(
        operator_allows_tool_execution,
        caller_allows_tool_execution,
    );
    resolve_source_route_with_policy(request, policy)
}

fn resolve_source_route_with_policy(
    request: &SourceRequest,
    policy: RouteSecurityPolicy,
) -> Result<RoutedSource, ApiError> {
    let components = route_components();
    let resolved = components.resolver.resolve(request)?;
    let route = components
        .router
        .route_with_policy(request, resolved, policy)?;
    let kind = route.source.source_kind;

    Ok(RoutedSource { kind, route })
}

pub(crate) async fn resolve_authorized_source_route(
    request: &SourceRequest,
    input: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    affinity: ExecutionAffinity,
    operator_allows_tool_execution: bool,
    allowed_roots: Option<&[std::path::PathBuf]>,
    event_emitter: SourceEventEmitter,
) -> Result<AuthorizedSourceRoute, ApiError> {
    event_emitter
        .running(PipelinePhase::Resolving, "resolving source request")
        .await;
    let routed = match resolve_source_route_for_access(
        request,
        auth_snapshot,
        operator_allows_tool_execution,
    ) {
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
    let adapter = route.adapter.clone();
    let event_emitter =
        event_emitter.with_route(route.source.source_kind, route.scope, adapter.clone());
    event_emitter
        .running(PipelinePhase::Routing, "routing source request")
        .await;
    event_emitter
        .running(PipelinePhase::Authorizing, "authorizing source request")
        .await;
    authorize_route_plan(
        &route,
        input,
        kind,
        auth_snapshot,
        affinity,
        allowed_roots,
        &event_emitter,
    )
    .await?;
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
    kind: SourceKind,
    auth_snapshot: Option<&AuthSnapshot>,
    affinity: ExecutionAffinity,
    allowed_roots: Option<&[std::path::PathBuf]>,
    event_emitter: &SourceEventEmitter,
) -> Result<(), ApiError> {
    match SourceAccessDecision::evaluate(route, input, kind, auth_snapshot, affinity, allowed_roots)
    {
        Ok(decision) => {
            tracing::debug!(
                source_kind = ?kind,
                required_scope = decision.required_scope.as_scope_str(),
                affinity = ?decision.affinity,
                local_root_enforced = decision.local_root_enforced,
                "source access decision allowed"
            );
            Ok(())
        }
        Err(err) => {
            event_emitter
                .failed(PipelinePhase::Authorizing, "source access decision denied")
                .await;
            Err(err)
        }
    }
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
