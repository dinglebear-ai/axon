//! REST schema registry entries for the Codex app-server control surface.

use super::{RestRouteSpec, job_admin, read};

pub(super) static CODEX_ROUTES: &[RestRouteSpec] = &[
    RestRouteSpec {
        required_scope: "admin",
        responses: &["200", "401", "403", "409", "503"],
        ..read(
            "GET",
            "/v1/synthesis/account",
            "synthesis_account_status",
            "AccountStatus",
        )
    },
    RestRouteSpec {
        responses: &["200", "400", "401", "403", "409", "415", "422", "503"],
        ..job_admin(
            "POST",
            "/v1/synthesis/account/login",
            "synthesis_account_login",
            Some("LoginBody"),
            "AccountStatus",
        )
    },
    RestRouteSpec {
        responses: &["200", "400", "401", "403", "404", "415", "422", "503"],
        ..job_admin(
            "POST",
            "/v1/synthesis/account/login/cancel",
            "synthesis_account_cancel",
            Some("CancelBody"),
            "AccountStatus",
        )
    },
    read("GET", "/v1/codex", "codex_snapshot", "CodexControlSnapshot"),
    read(
        "GET",
        "/v1/codex/events",
        "codex_events",
        "Vec<RecordedEvent>",
    ),
    read(
        "GET",
        "/v1/codex/{resource}",
        "codex_resource",
        "CodexResourceResponse",
    ),
    read(
        "GET",
        "/v1/codex/operations",
        "codex_operations",
        "Vec<ControlOperation>",
    ),
    RestRouteSpec {
        method: "POST",
        path: "/v1/codex/read",
        operation_id: "codex_read",
        request_dto: Some("CodexReadBody"),
        result_dto: "CodexResourceResponse",
        required_scope: "read",
        mutates: false,
        streaming: false,
        responses: super::READ_RESPONSES,
    },
    job_admin(
        "POST",
        "/v1/codex/operations",
        "codex_operation_create",
        Some("CreateOperationBody"),
        "ControlOperation",
    ),
    job_admin(
        "POST",
        "/v1/codex/operations/{id}/approve",
        "codex_operation_approve",
        None,
        "ApproveOperationResponse",
    ),
    job_admin(
        "POST",
        "/v1/codex/operations/{id}/cancel",
        "codex_operation_cancel",
        None,
        "ReconcileOperationResponse",
    ),
    job_admin(
        "POST",
        "/v1/codex/operations/{id}/execute",
        "codex_operation_execute",
        Some("ExecuteBody"),
        "ExecuteOperationResponse",
    ),
    job_admin(
        "POST",
        "/v1/codex/operations/{id}/reconcile",
        "codex_operation_reconcile",
        Some("ReconcileOperationBody"),
        "ReconcileOperationResponse",
    ),
    job_admin(
        "POST",
        "/v1/codex/server-requests/{id}/respond",
        "codex_server_request_respond",
        Some("ServerRequestResponseBody"),
        "ServerRequestRespondedResponse",
    ),
];
