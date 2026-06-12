use crate::services::types::{RestRouteAuth, rest_route_inventory};
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
        routing::v1_capabilities,
        handlers::discovery::sources,
        handlers::discovery::domains,
        handlers::discovery::stats,
        handlers::discovery::status,
        handlers::discovery::doctor,
        handlers::ask::v1_ask,
        handlers::ask_stream::v1_ask_stream,
        handlers::chat::v1_chat,
        handlers::chat_stream::v1_chat_stream,
        handlers::rag::query,
        handlers::rag::retrieve,
        handlers::rag::evaluate,
        handlers::rag::suggest,
        handlers::exploration::scrape,
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
        handlers::async_jobs::start_crawl,
        handlers::async_jobs::start_embed,
        handlers::async_jobs::start_extract,
        handlers::async_jobs::start_ingest,
        handlers::async_jobs::start_prepared_sessions_ingest,
        handlers::jobs::list_jobs,
        handlers::jobs::job_status,
        handlers::jobs::cancel_job,
        handlers::jobs::cleanup_jobs,
        handlers::jobs::clear_jobs,
        handlers::jobs::recover_jobs,
        openapi_jobs::list_embed_jobs,
        openapi_jobs::list_extract_jobs,
        openapi_jobs::list_ingest_jobs,
        openapi_jobs::embed_job_status,
        openapi_jobs::extract_job_status,
        openapi_jobs::ingest_job_status,
        openapi_jobs::cancel_embed_job,
        openapi_jobs::cancel_extract_job,
        openapi_jobs::cancel_ingest_job,
        openapi_jobs::cleanup_embed_jobs,
        openapi_jobs::cleanup_extract_jobs,
        openapi_jobs::cleanup_ingest_jobs,
        openapi_jobs::clear_embed_jobs,
        openapi_jobs::clear_extract_jobs,
        openapi_jobs::clear_ingest_jobs,
        openapi_jobs::recover_embed_jobs,
        openapi_jobs::recover_extract_jobs,
        openapi_jobs::recover_ingest_jobs,
        handlers::admin::dedupe,
        handlers::admin::list_watch,
        handlers::admin::create_watch,
        handlers::admin::run_watch,
        handlers::artifacts::serve_artifact_query
    ),
    components(schemas(
        crate::services::client_contract::RestAskRequest,
        crate::services::client_contract::RestChatRequest,
        crate::services::client_contract::RestChatResponse,
        crate::services::types::ServerInfo,
        super::error::ErrorKind,
        super::error::ErrorBody,
        crate::services::client_contract::RestQueryRequest,
        crate::services::client_contract::RestRetrieveRequest,
        crate::services::client_contract::RestEvaluateRequest,
        crate::services::client_contract::RestSuggestRequest,
        crate::services::client_contract::RestScrapeRequest,
        crate::services::client_contract::RestSummarizeRequest,
        crate::services::client_contract::RestMapRequest,
        handlers::exploration::EndpointsRequest,
        crate::services::client_contract::RestBrandRequest,
        crate::services::client_contract::RestDiffRequest,
        crate::services::client_contract::RestScreenshotRequest,
        crate::services::client_contract::RestMemoryRequest,
        crate::services::client_contract::RestMemorySubaction,
        crate::services::client_contract::RestMemoryNodeType,
        crate::services::client_contract::RestMemoryEdgeType,
        crate::services::types::BrandResult,
        crate::services::types::BrandColor,
        crate::services::types::ColorUsage,
        crate::services::types::LogoVariant,
        crate::services::types::DiffResult,
        crate::services::types::DiffStatus,
        crate::services::types::MetadataChange,
        crate::services::types::LinkEntry,
        crate::services::types::ScreenshotResult,
        crate::services::client_contract::RestSearchRequest,
        crate::services::client_contract::RestResearchRequest,
        crate::services::types::EndpointReport,
        crate::services::types::DiscoveredEndpoint,
        crate::services::types::EndpointVerification,
        crate::services::types::RpcProbeResult,
        crate::services::types::RpcProtocol,
        crate::services::types::RpcTransport,
        crate::services::types::EndpointKind,
        crate::services::types::EndpointSourceKind,
        crate::services::client_contract::RestCrawlRequest,
        crate::services::client_contract::RestEmbedRequest,
        crate::services::client_contract::RestExtractRequest,
        crate::services::client_contract::RestIngestRequest,
        crate::services::client_contract::RestSessionsIngestOptions,
        handlers::async_jobs::AcceptedJob,
        crate::ingest::sessions::PreparedSessionDoc,
        crate::ingest::sessions::IngestSessionsPreparedRequest,
        handlers::admin::DedupeRequest,
        handlers::admin::WatchCreateRequest
    )),
    tags(
        (name = "discovery", description = "Read-only source, domain, stats, status, and health endpoints"),
        (name = "rag", description = "Query, retrieve, ask, evaluate, and suggest endpoints"),
        (name = "exploration", description = "Scrape, summarize, map, search, and research endpoints"),
        (name = "jobs", description = "Async crawl, embed, extract, and ingest job endpoints"),
        (name = "admin", description = "Administrative mutation endpoints"),
        (name = "watch", description = "Scheduled watch definitions and runs"),
        (name = "memory", description = "Persistent agent memory endpoints")
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
        Content::new(Some(Ref::from_schema_name("ErrorBody"))),
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
