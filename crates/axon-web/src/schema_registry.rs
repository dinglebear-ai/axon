//! REST route registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestRouteSpec {
    pub method: &'static str,
    pub path: &'static str,
    pub operation_id: &'static str,
    pub request_dto: Option<&'static str>,
    pub result_dto: &'static str,
    pub required_scope: &'static str,
    pub mutates: bool,
    pub streaming: bool,
    pub responses: &'static [&'static str],
}

pub fn rest_route_registry() -> &'static [RestRouteSpec] {
    REST_ROUTES
}

static REST_ROUTES: &[RestRouteSpec] = &[
    read(
        "GET",
        "/v1/capabilities",
        "capabilities",
        "CapabilitiesResponse",
    ),
    read("GET", "/v1/sources", "sources", "SourceListResponse"),
    read("GET", "/v1/domains", "domains", "DomainListResponse"),
    read("GET", "/v1/stats", "stats", "StatsResponse"),
    read("GET", "/v1/status", "status", "StatusResponse"),
    read("GET", "/v1/doctor", "doctor", "DoctorResponse"),
    read(
        "GET",
        "/v1/collections",
        "collections",
        "CollectionsResponse",
    ),
    read(
        "GET",
        "/v1/mobile/sessions",
        "mobile_sessions",
        "MobileSessionListResponse",
    ),
    read(
        "GET",
        "/v1/mobile/sessions/{id}",
        "mobile_session",
        "MobileSessionResponse",
    ),
    write(
        "PUT",
        "/v1/mobile/sessions/{id}",
        "upsert_mobile_session",
        Some("UpsertMobileSessionRequest"),
        "UpsertMobileSessionResponse",
    ),
    write(
        "DELETE",
        "/v1/mobile/sessions/{id}",
        "delete_mobile_session",
        None,
        "DeleteMobileSessionResponse",
    ),
    RestRouteSpec {
        method: "POST",
        path: "/v1/ask",
        operation_id: "ask",
        request_dto: Some("AskRequest"),
        result_dto: "AskResponse",
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: ASK_RESPONSES,
    },
    stream(
        "POST",
        "/v1/ask/stream",
        "ask_stream",
        Some("AskRequest"),
        "AskStreamEvent",
    ),
    RestRouteSpec {
        method: "POST",
        path: "/v1/chat",
        operation_id: "chat",
        request_dto: Some("ChatRequest"),
        result_dto: "ChatResponse",
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: ASK_RESPONSES,
    },
    stream(
        "POST",
        "/v1/chat/stream",
        "chat_stream",
        Some("ChatRequest"),
        "ChatStreamEvent",
    ),
    RestRouteSpec {
        method: "POST",
        path: "/v1/query",
        operation_id: "query",
        request_dto: Some("VectorSearchRequest"),
        result_dto: "VectorSearchResult",
        required_scope: "read",
        mutates: false,
        streaming: false,
        responses: READ_RESPONSES,
    },
    RestRouteSpec {
        method: "POST",
        path: "/v1/retrieve",
        operation_id: "retrieve",
        request_dto: Some("RetrieveRequest"),
        result_dto: "RetrieveResponse",
        required_scope: "read",
        mutates: false,
        streaming: false,
        responses: READ_RESPONSES,
    },
    RestRouteSpec {
        method: "POST",
        path: "/v1/search",
        operation_id: "search",
        request_dto: Some("SearchRequest"),
        result_dto: "SearchResponse",
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: SYNC_WRITE_RESPONSES,
    },
    RestRouteSpec {
        method: "POST",
        path: "/v1/research",
        operation_id: "research",
        request_dto: Some("ResearchRequest"),
        result_dto: "ResearchResponse",
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: SYNC_WRITE_RESPONSES,
    },
    RestRouteSpec {
        method: "POST",
        path: "/v1/map",
        operation_id: "map",
        request_dto: Some("MapRequest"),
        result_dto: "MapResponse",
        required_scope: "read",
        mutates: false,
        streaming: false,
        responses: READ_RESPONSES,
    },
    write(
        "POST",
        "/v1/endpoints",
        "endpoints",
        Some("EndpointRequest"),
        "EndpointResponse",
    ),
    write(
        "POST",
        "/v1/brand",
        "brand",
        Some("BrandRequest"),
        "BrandResponse",
    ),
    write(
        "POST",
        "/v1/diff",
        "diff",
        Some("DiffRequest"),
        "DiffResponse",
    ),
    write(
        "POST",
        "/v1/screenshot",
        "screenshot",
        Some("ScreenshotRequest"),
        "ScreenshotResponse",
    ),
    write(
        "POST",
        "/v1/evaluate",
        "evaluate",
        Some("EvaluateRequest"),
        "EvaluateResponse",
    ),
    write(
        "POST",
        "/v1/suggest",
        "suggest",
        Some("SuggestRequest"),
        "SuggestResponse",
    ),
    write(
        "POST",
        "/v1/sources",
        "create_source",
        Some("SourceRequest"),
        "SourceResult",
    ),
    write(
        "POST",
        "/v1/summarize",
        "summarize",
        Some("SummarizeRequest"),
        "SummarizeResponse",
    ),
    stream(
        "POST",
        "/v1/summarize/stream",
        "summarize_stream",
        Some("SummarizeRequest"),
        "SummarizeStreamEvent",
    ),
    stream(
        "POST",
        "/v1/research/stream",
        "research_stream",
        Some("ResearchRequest"),
        "ResearchStreamEvent",
    ),
    write(
        "POST",
        "/v1/memory",
        "memory",
        Some("MemoryRequest"),
        "MemoryResponse",
    ),
    read("GET", "/v1/artifacts", "artifacts", "ArtifactQueryResponse"),
    job_read("GET", "/v1/jobs", "jobs_list", "JobListPage"),
    job_read("GET", "/v1/jobs/{id}", "jobs_status", "JobSummary"),
    job_read("GET", "/v1/jobs/{id}/events", "jobs_events", "JobEventPage"),
    RestRouteSpec {
        method: "GET",
        path: "/v1/jobs/{id}/stream",
        operation_id: "jobs_stream",
        request_dto: None,
        result_dto: "StreamEvent",
        required_scope: "read",
        mutates: false,
        streaming: true,
        responses: READ_RESPONSES,
    },
    job_read(
        "GET",
        "/v1/jobs/{id}/artifacts",
        "jobs_artifacts",
        "JobArtifactListResult",
    ),
    job_admin(
        "DELETE",
        "/v1/jobs",
        "jobs_clear",
        Some("JobClearRequest"),
        "JobClearResult",
    ),
    job_write(
        "POST",
        "/v1/jobs/{id}/cancel",
        "jobs_cancel",
        Some("JobCancelRequest"),
        "JobCancelResult",
    ),
    job_write(
        "POST",
        "/v1/jobs/{id}/retry",
        "jobs_retry",
        Some("JobRetryRequest"),
        "JobRetryResult",
    ),
    job_admin(
        "POST",
        "/v1/jobs/recover",
        "jobs_recover",
        Some("JobRecoveryRequest"),
        "JobRecoveryResult",
    ),
    job_admin(
        "POST",
        "/v1/jobs/cleanup",
        "jobs_cleanup",
        Some("JobCleanupRequest"),
        "JobCleanupResult",
    ),
    job_read("GET", "/v1/extract", "extract_list", "JobListResponse"),
    job_write(
        "POST",
        "/v1/extract",
        "extract",
        Some("ExtractRequest"),
        "JobDescriptor",
    ),
    job_write(
        "DELETE",
        "/v1/extract",
        "extract_clear",
        None,
        "JobCleanupResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/cleanup",
        "extract_cleanup",
        None,
        "JobCleanupResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/recover",
        "extract_recover",
        None,
        "JobRecoveryResponse",
    ),
    job_read(
        "GET",
        "/v1/extract/{id}",
        "extract_status",
        "JobStatusResponse",
    ),
    job_write(
        "POST",
        "/v1/extract/{id}/cancel",
        "extract_cancel",
        None,
        "JobCancelResponse",
    ),
    write(
        "POST",
        "/v1/dedupe",
        "dedupe",
        Some("DedupeRequest"),
        "DedupeResponse",
    ),
    write(
        "POST",
        "/v1/purge",
        "purge",
        Some("PurgeRequest"),
        "PurgeResult",
    ),
    read("GET", "/v1/watch", "watch_list", "WatchListResponse"),
    write(
        "POST",
        "/v1/watch",
        "watch_create",
        Some("WatchRequest"),
        "WatchResponse",
    ),
    accepted(
        "POST",
        "/v1/watch/{id}/run",
        "watch_run",
        None,
        "WatchRunResponse",
    ),
];

pub fn removed_routes() -> &'static [&'static str] {
    &["/v1/embed", "/v1/ingest", "/v1/scrape", "/v1/crawl"]
}

const READ_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "500", "502"];
const ASK_RESPONSES: &[&str] = &["200", "400", "401", "403", "413", "502", "504"];
const SYNC_WRITE_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "500", "502", "504"];
const WRITE_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "422", "500", "502"];
const ACCEPTED_RESPONSES: &[&str] = &["202", "400", "401", "403", "404", "500", "502"];
const STREAM_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "500", "502"];

const fn read(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    result_dto: &'static str,
) -> RestRouteSpec {
    RestRouteSpec {
        method,
        path,
        operation_id,
        request_dto: None,
        result_dto,
        required_scope: "read",
        mutates: false,
        streaming: false,
        responses: READ_RESPONSES,
    }
}

const fn write(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    RestRouteSpec {
        method,
        path,
        operation_id,
        request_dto,
        result_dto,
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: WRITE_RESPONSES,
    }
}

const fn accepted(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    RestRouteSpec {
        method,
        path,
        operation_id,
        request_dto,
        result_dto,
        required_scope: "write",
        mutates: true,
        streaming: false,
        responses: ACCEPTED_RESPONSES,
    }
}

const fn stream(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    RestRouteSpec {
        method,
        path,
        operation_id,
        request_dto,
        result_dto,
        required_scope: "write",
        mutates: true,
        streaming: true,
        responses: STREAM_RESPONSES,
    }
}

const fn job_read(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    result_dto: &'static str,
) -> RestRouteSpec {
    read(method, path, operation_id, result_dto)
}

const fn job_write(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    write(method, path, operation_id, request_dto, result_dto)
}

const fn job_admin(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    RestRouteSpec {
        method,
        path,
        operation_id,
        request_dto,
        result_dto,
        required_scope: "admin",
        mutates: true,
        streaming: false,
        responses: WRITE_RESPONSES,
    }
}
