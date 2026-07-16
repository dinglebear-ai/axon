//! Constructors and shared response sets for REST schema declarations.

use super::RestRouteSpec;

pub(super) const READ_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "500", "502"];
pub(super) const ASK_RESPONSES: &[&str] = &["200", "400", "401", "403", "413", "502", "504"];
pub(super) const SYNC_WRITE_RESPONSES: &[&str] =
    &["200", "400", "401", "403", "404", "500", "502", "504"];
pub(super) const WRITE_RESPONSES: &[&str] =
    &["200", "400", "401", "403", "404", "422", "500", "502"];
pub(super) const STREAM_RESPONSES: &[&str] = &["200", "400", "401", "403", "404", "500", "502"];

pub(super) const fn read(
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

pub(super) const fn write(
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

/// Like [`write`], but gated `axon:read` — for query-shaped surfaces
/// (evaluate/suggest/summarize/memory search/context, U2-20/C6-20) that may
/// still enqueue a background job as a side effect (`mutates: true`)
/// without requiring `axon:write` to invoke.
pub(super) const fn read_query_surface(
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
        required_scope: "read",
        mutates: true,
        streaming: false,
        responses: WRITE_RESPONSES,
    }
}

pub(super) const fn stream(
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

pub(super) const fn job_read(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    result_dto: &'static str,
) -> RestRouteSpec {
    read(method, path, operation_id, result_dto)
}

pub(super) const fn job_write(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    request_dto: Option<&'static str>,
    result_dto: &'static str,
) -> RestRouteSpec {
    write(method, path, operation_id, request_dto, result_dto)
}

pub(super) const fn job_admin(
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
