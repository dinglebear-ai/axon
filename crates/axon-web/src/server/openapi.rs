use axon_services::types::{RestRouteAuth, rest_route_inventory};
use utoipa::OpenApi;
use utoipa::openapi::security::{
    AuthorizationCode, Flow, HttpAuthScheme, HttpBuilder, OAuth2, Scopes, SecurityRequirement,
    SecurityScheme,
};
use utoipa::openapi::{
    Content, Ref, RefOr,
    path::Operation,
    response::{Response, Responses},
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use super::{handlers, openapi_jobs, routing};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Axon REST API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Dedicated REST routes for Axon discovery, RAG, crawl, ingest, and watch workflows."
    ),
    paths(
        super::super::health::healthz,
        super::super::health::readyz,
        routing::v1_capabilities,
        handlers::discovery::sources,
        handlers::discovery::domains,
        handlers::discovery::stats,
        handlers::discovery::status,
        handlers::discovery::doctor,
        handlers::config::collections_openapi_marker,
        handlers::ask::v1_ask,
        handlers::ask_stream::v1_ask_stream,
        handlers::chat::v1_chat,
        handlers::chat_stream::v1_chat_stream,
        handlers::rag::query,
        handlers::rag::retrieve,
        handlers::rag::evaluate,
        handlers::rag::suggest,
        handlers::sources::index_source,
        handlers::exploration::summarize,
        handlers::exploration::exploration_stream::summarize_stream,
        handlers::exploration::map,
        handlers::exploration::endpoints,
        handlers::exploration::brand,
        handlers::exploration::diff,
        handlers::exploration::screenshot,
        handlers::exploration::search,
        handlers::exploration::research,
        handlers::exploration::exploration_stream::research_stream,
        handlers::memory::memory,
        handlers::mobile_sessions::list_mobile_sessions,
        handlers::mobile_sessions::get_mobile_session,
        handlers::mobile_sessions::upsert_mobile_session,
        handlers::mobile_sessions::delete_mobile_session,
        handlers::async_jobs::start_extract,
        handlers::jobs::list_unified_jobs,
        handlers::jobs::unified_job_status,
        handlers::jobs::unified_job_events,
        handlers::jobs_stream::unified_job_stream,
        handlers::jobs::unified_job_artifacts,
        handlers::jobs::cancel_unified_job,
        handlers::jobs::retry_unified_job,
        handlers::jobs::recover_unified_jobs,
        handlers::jobs::cleanup_unified_jobs,
        handlers::jobs::clear_unified_jobs,
        openapi_jobs::list_extract_jobs,
        openapi_jobs::extract_job_status,
        openapi_jobs::cancel_extract_job,
        openapi_jobs::cleanup_extract_jobs,
        openapi_jobs::clear_extract_jobs,
        openapi_jobs::recover_extract_jobs,
        handlers::admin::dedupe,
        handlers::admin::purge,
        handlers::admin::list_watch,
        handlers::admin::create_watch,
        handlers::admin::run_watch,
        handlers::artifacts::serve_artifact_query
    ),
    components(schemas(
        super::super::health::ReadinessBody,
        axon_services::client_contract::RestAskRequest,
        axon_services::client_contract::RestChatRequest,
        axon_services::client_contract::RestChatResponse,
        axon_services::types::ServerInfo,
        super::types::PanelCollectionsResponse,
        super::error::ErrorKind,
        super::error::ErrorBody,
        axon_api::source::ErrorEnvelope,
        axon_api::ApiError,
        axon_error::ErrorCode,
        axon_error::ErrorStage,
        axon_error::ErrorSeverity,
        axon_error::ErrorVisibility,
        axon_services::client_contract::RestQueryRequest,
        axon_services::client_contract::RestRetrieveRequest,
        axon_services::client_contract::RestEvaluateRequest,
        axon_services::client_contract::RestSuggestRequest,
        axon_api::source::SourceRequest,
        axon_api::source::SourceResult,
        axon_services::client_contract::RestSummarizeRequest,
        axon_services::client_contract::RestMapRequest,
        handlers::exploration::EndpointsRequest,
        axon_services::client_contract::RestBrandRequest,
        axon_services::client_contract::RestDiffRequest,
        axon_services::client_contract::RestScreenshotRequest,
        axon_services::client_contract::RestMemoryRequest,
        axon_services::client_contract::RestMemorySubaction,
        axon_services::client_contract::RestMemoryNodeType,
        axon_services::client_contract::RestMemoryEdgeType,
        axon_services::mobile_sessions::MobileChatItem,
        axon_services::mobile_sessions::MobileSession,
        axon_services::mobile_sessions::MobileSessionSummary,
        axon_services::mobile_sessions::MobileSessionListResponse,
        axon_services::mobile_sessions::MobileSessionDetailResponse,
        axon_services::mobile_sessions::UpsertMobileSessionRequest,
        axon_services::mobile_sessions::UpsertMobileSessionResponse,
        axon_services::mobile_sessions::DeleteMobileSessionResponse,
        axon_services::types::BrandResult,
        axon_services::types::BrandColor,
        axon_services::types::ColorUsage,
        axon_services::types::LogoVariant,
        axon_services::types::DiffResult,
        axon_services::types::DiffStatus,
        axon_services::types::MetadataChange,
        axon_services::types::LinkEntry,
        axon_services::types::ScreenshotResult,
        axon_services::client_contract::RestSearchRequest,
        axon_services::client_contract::RestResearchRequest,
        axon_services::types::EndpointReport,
        axon_services::types::DiscoveredEndpoint,
        axon_services::types::EndpointVerification,
        axon_services::types::RpcProbeResult,
        axon_services::types::RpcProtocol,
        axon_services::types::RpcTransport,
        axon_services::types::EndpointKind,
        axon_services::types::EndpointSourceKind,
        axon_services::client_contract::RestExtractRequest,
        handlers::async_jobs::AcceptedJob,
        handlers::admin::DedupeRequest,
        axon_api::mcp_schema::PurgeRequest,
        axon_api::PurgeResult,
        handlers::admin::WatchCreateRequest,
        handlers::jobs::JobStatusResponse,
        axon_api::source::JobCancelRequest,
        axon_api::source::JobCleanupRequest,
        axon_api::source::JobCleanupResult,
        axon_api::source::JobClearRequest,
        axon_api::source::JobClearResult,
        axon_api::source::JobEventPage,
        axon_api::source::JobRecoveryRequest,
        axon_api::source::JobRecoveryResult,
        axon_api::source::JobRetryRequest,
        axon_api::source::JobRetryResult,
        axon_api::source::JobSummary,
        axon_api::source::StreamEvent,
        axon_api::source::JobDescriptor,
        axon_api::source::JobArtifactListResult,
        axon_api::job_progress::JobProgress,
        axon_api::job_progress::JobFamily,
        axon_api::job_progress::JobPhase,
        axon_api::job_progress::JobMetric
    )),
    tags(
        (name = "discovery", description = "Read-only source, domain, stats, status, and health endpoints"),
        (name = "rag", description = "Query, retrieve, ask, evaluate, and suggest endpoints"),
        (name = "exploration", description = "Summarize, map, search, and research endpoints"),
        (name = "sources", description = "Unified source indexing entrypoint"),
        (name = "jobs", description = "Async extract job endpoints"),
        (name = "admin", description = "Administrative mutation endpoints"),
        (name = "watch", description = "Scheduled watch definitions and runs"),
        (name = "memory", description = "Persistent agent memory endpoints"),
        (name = "mobile", description = "Mobile app session synchronization endpoints")
    )
)]
struct ApiDoc;

pub fn openapi_document() -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    apply_security_metadata(&mut document);
    document
}

fn apply_security_metadata(document: &mut utoipa::openapi::OpenApi) {
    let components = document.components.get_or_insert_with(Default::default);
    components.add_security_scheme(
        "bearerAuth",
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("JWT or static token")
                .build(),
        ),
    );
    components.add_security_scheme(
        "oauth2",
        SecurityScheme::OAuth2(OAuth2::new([Flow::AuthorizationCode(
            AuthorizationCode::new(
                "/authorize",
                "/token",
                Scopes::from_iter([
                    ("axon:read", "Authenticated Axon REST access"),
                    ("axon:write", "Authenticated Axon REST access"),
                ]),
            ),
        )])),
    );

    for route in rest_route_inventory()
        .iter()
        .filter(|route| route.openapi && route.auth != RestRouteAuth::Public)
    {
        let Some(operation) = operation_mut(document, route.path, route.method) else {
            continue;
        };
        operation.security = Some(vec![
            SecurityRequirement::new("bearerAuth", Vec::<&str>::new()),
            SecurityRequirement::new("oauth2", ["axon:read"]),
            SecurityRequirement::new("oauth2", ["axon:write"]),
        ]);
        add_auth_error_responses(&mut operation.responses);
    }
}

fn add_auth_error_responses(responses: &mut Responses) {
    responses
        .responses
        .entry("401".to_string())
        .or_insert_with(|| auth_error_response("Missing or invalid authentication"));
    responses
        .responses
        .entry("403".to_string())
        .or_insert_with(|| auth_error_response("Authenticated token lacks Axon access"));
}

fn auth_error_response(description: &'static str) -> RefOr<Response> {
    let mut response = Response::new(description);
    response.content.insert(
        "application/json".to_string(),
        Content::new(Some(Ref::from_schema_name("ErrorEnvelope"))),
    );
    RefOr::T(response)
}

fn operation_mut<'a>(
    document: &'a mut utoipa::openapi::OpenApi,
    path: &str,
    method: &str,
) -> Option<&'a mut Operation> {
    let path_item = document.paths.paths.get_mut(path)?;
    match method {
        "GET" => path_item.get.as_mut(),
        "POST" => path_item.post.as_mut(),
        "PUT" => path_item.put.as_mut(),
        "DELETE" => path_item.delete.as_mut(),
        _ => None,
    }
}

pub(super) fn docs_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let (_, openapi) = OpenApiRouter::<S>::with_openapi(openapi_document()).split_for_parts();
    SwaggerUi::new("/docs")
        .url("/api-docs/openapi.json", openapi)
        .into()
}
