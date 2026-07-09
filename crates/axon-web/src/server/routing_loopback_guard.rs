//! `LoopbackDev` destructive-route guard.
//!
//! When no auth is configured and the server is bound to loopback only
//! (`AuthPolicy::LoopbackDev`), most REST routes are reachable without a
//! token — the loopback bind itself is the trust boundary. Destructive
//! operations are the exception: they still require configured auth even on
//! loopback, so an accidental non-loopback expose (e.g. a port-forward) can't
//! silently turn a dev box into an open destructive API. This module owns the
//! path/method classification for that exception list.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    middleware,
    response::{IntoResponse, Response},
};

use super::super::error::HttpError;

pub(super) async fn block_loopback_destructive_request(
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    if is_loopback_destructive_request(request.method(), request.uri().path()) {
        return HttpError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "destructive REST route requires configured auth",
        )
        .into_response();
    }
    next.run(request).await
}

fn is_loopback_destructive_request(method: &Method, path: &str) -> bool {
    if *method == Method::POST
        && (path == "/v1/dedupe"
            || path == "/v1/purge"
            || path == "/v1/sources"
            || path == "/v1/watch"
            || path == "/v1/jobs/recover"
            || path == "/v1/jobs/cleanup"
            || path == "/v1/prune/plan"
            || path == "/v1/prune/exec"
            || path.starts_with("/v1/watch/")
            || path.starts_with("/v1/jobs/"))
    {
        return true;
    }
    if *method == Method::DELETE && path == "/v1/jobs" {
        return true;
    }
    if *method == Method::POST && path == "/v1/memory" {
        return true;
    }
    if is_memory_write(method, path) {
        return true;
    }
    if is_mobile_session_write(method, path) {
        return true;
    }

    let prefix = "/v1/extract";
    if path == prefix {
        return *method == Method::POST || *method == Method::DELETE;
    }
    if let Some(remainder) = path
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        && *method == Method::POST
        && (remainder == "cleanup" || remainder == "recover" || remainder.ends_with("/cancel"))
    {
        return true;
    }
    false
}

/// All mutating per-verb `/v1/memories*` routes — the same loopback-dev
/// destructive-route treatment the old `POST /v1/memory` passthrough got.
/// `GET /v1/memories/{memory_id}` (show) is intentionally excluded — it's a
/// pure read, registered in `read_routes`.
fn is_memory_write(method: &Method, path: &str) -> bool {
    if path == "/v1/memories" {
        return *method == Method::POST;
    }
    if path == "/v1/memories/import" || path == "/v1/memories/export" {
        return *method == Method::POST;
    }
    let Some(remainder) = path.strip_prefix("/v1/memories/") else {
        return false;
    };
    match remainder {
        "search" | "context" | "review" | "compact" => *method == Method::POST,
        other => {
            if *method == Method::DELETE {
                return true;
            }
            *method == Method::POST
                && (other.ends_with("/link")
                    || other.ends_with("/supersede")
                    || other.ends_with("/reinforce")
                    || other.ends_with("/contradict")
                    || other.ends_with("/pin")
                    || other.ends_with("/archive")
                    || other.ends_with("/compact"))
        }
    }
}

fn is_mobile_session_write(method: &Method, path: &str) -> bool {
    (*method == Method::PUT || *method == Method::DELETE)
        && path
            .strip_prefix("/v1/mobile/sessions/")
            .is_some_and(|id| !id.is_empty())
}
