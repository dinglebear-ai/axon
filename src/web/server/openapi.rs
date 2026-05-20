use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use super::{handlers, openapi_jobs};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Axon REST API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Dedicated REST routes for Axon discovery, RAG, crawl, ingest, and watch workflows."
    ),
    paths(
        handlers::discovery::sources,
        handlers::discovery::domains,
        handlers::discovery::stats,
        handlers::discovery::status,
        handlers::discovery::doctor,
        handlers::ask::v1_ask,
        handlers::rag::query,
        handlers::rag::retrieve,
        handlers::rag::evaluate,
        handlers::rag::suggest,
        handlers::exploration::scrape,
        handlers::exploration::summarize,
        handlers::exploration::map,
        handlers::exploration::endpoints,
        handlers::exploration::search,
        handlers::exploration::research,
        handlers::async_jobs::start_crawl,
        handlers::async_jobs::start_embed,
        handlers::async_jobs::start_extract,
        handlers::async_jobs::start_ingest,
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
        handlers::admin::run_watch
    ),
    components(schemas(
        super::types::AskRequestBody,
        super::error::ErrorBody,
        handlers::rag::QueryRequest,
        handlers::rag::RetrieveRequest,
        handlers::rag::EvaluateRequest,
        handlers::rag::SuggestRequest,
        handlers::exploration::ScrapeRequest,
        handlers::exploration::SummarizeRequest,
        handlers::exploration::MapRequest,
        handlers::exploration::EndpointsRequest,
        handlers::exploration::SearchRequest,
        handlers::exploration::ResearchRequest,
        crate::services::types::EndpointReport,
        crate::services::types::DiscoveredEndpoint,
        crate::services::types::EndpointVerification,
        crate::services::types::EndpointKind,
        crate::services::types::EndpointSourceKind,
        handlers::async_jobs::CrawlStartRequest,
        handlers::async_jobs::EmbedStartRequest,
        handlers::async_jobs::ExtractStartRequest,
        handlers::async_jobs::AcceptedJob,
        handlers::admin::DedupeRequest,
        handlers::admin::WatchCreateRequest
    )),
    tags(
        (name = "discovery", description = "Read-only source, domain, stats, status, and health endpoints"),
        (name = "rag", description = "Query, retrieve, ask, evaluate, and suggest endpoints"),
        (name = "exploration", description = "Scrape, summarize, map, search, and research endpoints"),
        (name = "jobs", description = "Async crawl, embed, extract, and ingest job endpoints"),
        (name = "admin", description = "Administrative mutation endpoints"),
        (name = "watch", description = "Scheduled watch definitions and runs")
    )
)]
struct ApiDoc;

pub(super) fn docs_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let (_, openapi) = OpenApiRouter::<S>::with_openapi(ApiDoc::openapi()).split_for_parts();
    SwaggerUi::new("/docs")
        .url("/api-docs/openapi.json", openapi)
        .into()
}
