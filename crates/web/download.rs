//! HTTP download handlers for crawl results.
//!
//! Four routes:
//! - `GET /download/{job_id}/pack.md`  — Repomix-style packed Markdown
//! - `GET /download/{job_id}/pack.xml` — Repomix-style packed XML
//! - `GET /download/{job_id}/archive.zip` — ZIP of all markdown files
//! - `GET /download/{job_id}/file/*path` — Single file download

mod archive;
mod manifest;
mod validation;

use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use self::archive::build_zip;
use self::manifest::load_all_files;
use self::validation::{sanitize_filename, validate_job_dir};
use super::DownloadAuthState;
use super::pack;
use super::tailscale_auth::{AuthOutcome, check_auth};

/// Query parameters for download routes — mirrors `WsQuery` in `web.rs`.
#[derive(Deserialize)]
pub(crate) struct DownloadQuery {
    /// Retained for future signed-token support (HMAC-based, scoped, with expiry).
    /// Currently unused — download auth is header-only.
    #[allow(dead_code)]
    token: Option<String>,
}

/// Authenticate a download request with the shared API token.
///
/// Downloads require header-based auth only (Bearer or x-api-key).
/// The `?token=` query parameter is intentionally NOT accepted here because
/// the shared API token would leak into browser history, copied links, server
/// access logs, and `Referer` headers. (Thread 12 / PRRT_kwDORS2O8s50RyMu)
///
/// If browser-initiated downloads without header auth are needed in the future,
/// the right approach is short-lived signed download tokens (HMAC-based, scoped
/// to a specific job_id, with expiry) — not reusing the long-lived shared token.
fn auth_download(headers: &HeaderMap, state: &DownloadAuthState) -> AuthOutcome {
    // Pass None for query_token — only header-based auth is accepted for downloads.
    check_auth(headers, None, state.api_token.as_deref())
}

/// `GET /download/{job_id}/pack.md`
pub async fn serve_pack_md(
    AxumPath(job_id): AxumPath<String>,
    headers: HeaderMap,
    Query(_params): Query<DownloadQuery>,
    State(state): State<Arc<DownloadAuthState>>,
) -> Response {
    if matches!(auth_download(&headers, &state), AuthOutcome::Denied(_)) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let job_dir = match validate_job_dir(&state.job_dirs, &job_id).await {
        Ok(d) => d,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let (domain, entries) = match load_all_files(&job_dir).await {
        Ok(v) => v,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let body = pack::build_pack_md(&domain, &entries);
    let safe_filename = sanitize_filename(&format!("{domain}-pack.md"));

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    resp_headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{safe_filename}\"")
            .parse::<header::HeaderValue>()
            .unwrap_or_else(|_| {
                header::HeaderValue::from_static("attachment; filename=\"download\"")
            }),
    );

    (resp_headers, body).into_response()
}

/// `GET /download/{job_id}/pack.xml`
pub async fn serve_pack_xml(
    AxumPath(job_id): AxumPath<String>,
    headers: HeaderMap,
    Query(_params): Query<DownloadQuery>,
    State(state): State<Arc<DownloadAuthState>>,
) -> Response {
    if matches!(auth_download(&headers, &state), AuthOutcome::Denied(_)) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let job_dir = match validate_job_dir(&state.job_dirs, &job_id).await {
        Ok(d) => d,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let (domain, entries) = match load_all_files(&job_dir).await {
        Ok(v) => v,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let body = pack::build_pack_xml(&domain, &entries);
    let safe_filename = sanitize_filename(&format!("{domain}-pack.xml"));

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    resp_headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{safe_filename}\"")
            .parse::<header::HeaderValue>()
            .unwrap_or_else(|_| {
                header::HeaderValue::from_static("attachment; filename=\"download\"")
            }),
    );

    (resp_headers, body).into_response()
}

/// `GET /download/{job_id}/archive.zip`
pub async fn serve_zip(
    AxumPath(job_id): AxumPath<String>,
    headers: HeaderMap,
    Query(_params): Query<DownloadQuery>,
    State(state): State<Arc<DownloadAuthState>>,
) -> Response {
    if matches!(auth_download(&headers, &state), AuthOutcome::Denied(_)) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let job_dir = match validate_job_dir(&state.job_dirs, &job_id).await {
        Ok(d) => d,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let (domain, entries) = match load_all_files(&job_dir).await {
        Ok(v) => v,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    // Capture filename before moving domain into the blocking closure
    let filename = format!("{domain}-crawl.zip");
    let zip_result = tokio::task::spawn_blocking(move || build_zip(&domain, &entries)).await;

    match zip_result {
        Ok(Ok(bytes)) => {
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/zip"),
            );
            let safe_filename = sanitize_filename(&filename);
            resp_headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe_filename}\"")
                    .parse::<header::HeaderValue>()
                    .unwrap_or_else(|_| {
                        header::HeaderValue::from_static("attachment; filename=\"download\"")
                    }),
            );
            (resp_headers, bytes).into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("zip creation failed: {e}"),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("zip task panicked: {e}"),
        )
            .into_response(),
    }
}

/// `GET /download/{job_id}/file/{path}`
pub async fn serve_file(
    AxumPath((job_id, file_path)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(_params): Query<DownloadQuery>,
    State(state): State<Arc<DownloadAuthState>>,
) -> Response {
    if matches!(auth_download(&headers, &state), AuthOutcome::Denied(_)) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let job_dir = match validate_job_dir(&state.job_dirs, &job_id).await {
        Ok(d) => d,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    // Reject obvious traversal attempts before touching the filesystem.
    // Uses Path::components() instead of substring matching so that valid filenames
    // containing ".." (e.g. "report..json") are not incorrectly rejected.
    if std::path::Path::new(&file_path)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
        || file_path.contains('\0')
    {
        return (StatusCode::BAD_REQUEST, "invalid file path").into_response();
    }

    let full_path = job_dir.join(&file_path);

    // Canonicalize both paths and verify containment
    let Ok(canonical_base) = tokio::fs::canonicalize(&job_dir).await else {
        return (StatusCode::NOT_FOUND, "job directory not found").into_response();
    };
    let Ok(canonical_file) = tokio::fs::canonicalize(&full_path).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };

    if !canonical_file.starts_with(&canonical_base) {
        return (StatusCode::FORBIDDEN, "path outside job directory").into_response();
    }

    let content = match tokio::fs::read_to_string(&canonical_file).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::NOT_FOUND, "file not found").into_response(),
    };

    let filename = canonical_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download.md".to_string());

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    let safe_filename = sanitize_filename(&filename);
    resp_headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{safe_filename}\"")
            .parse::<header::HeaderValue>()
            .unwrap_or_else(|_| {
                header::HeaderValue::from_static("attachment; filename=\"download\"")
            }),
    );

    (resp_headers, content).into_response()
}
