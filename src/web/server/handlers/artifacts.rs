//! Artifact file serving — `GET /v1/artifacts/{*path}`
//!
//! Serves files from `cfg.output_dir` with content-type inference.
//! Path traversal and symlink escapes are rejected before any I/O.

use crate::web::server::error::HttpError;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::{path::Path as FsPath, sync::Arc};

/// `(AppState, Arc<Config>)` state shape used by all exploration/artifact handlers.
type WebState = (
    super::super::state::AppState,
    Arc<crate::core::config::Config>,
);

/// Serve an artifact file from the configured output directory.
///
/// The `*path` wildcard is validated structurally and via canonicalization
/// before any file I/O. Returns:
/// - `400` for structurally unsafe paths (absolute, `..`, etc.)
/// - `403` when the resolved path escapes the output root or is a symlink
/// - `404` when the file does not exist or is not a regular file
#[utoipa::path(
    get,
    path = "/v1/artifacts/{path}",
    params(
        ("path" = String, Path, description = "Relative path within the output directory")
    ),
    responses(
        (status = 200, description = "File bytes with inferred content-type"),
        (status = 400, description = "Structurally unsafe path",
         body = crate::web::server::error::ErrorBody),
        (status = 403, description = "Path escapes the output root",
         body = crate::web::server::error::ErrorBody),
        (status = 404, description = "Artifact not found",
         body = crate::web::server::error::ErrorBody),
    ),
    tag = "artifacts"
)]
pub(crate) async fn serve_artifact(
    State((_state, cfg)): State<WebState>,
    Path(raw_path): Path<String>,
) -> Result<Response, HttpError> {
    if is_structurally_unsafe(&raw_path) {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "path contains traversal components or is absolute",
        ));
    }

    let root = &cfg.output_dir;
    let candidate = root.join(&raw_path);

    let canonical_root = tokio::fs::canonicalize(root).await.map_err(|_| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "output_dir_error",
            "output directory is not accessible",
        )
    })?;

    let canonical_candidate = tokio::fs::canonicalize(&candidate).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("artifact not found: {raw_path}"),
            )
        } else {
            HttpError::new(
                StatusCode::FORBIDDEN,
                "path_error",
                "cannot resolve artifact path",
            )
        }
    })?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(HttpError::new(
            StatusCode::FORBIDDEN,
            "path_escape",
            "path escapes the output root",
        ));
    }

    // Use symlink_metadata so symlinks are never followed silently.
    let meta = tokio::fs::symlink_metadata(&canonical_candidate)
        .await
        .map_err(|_| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("artifact not found: {raw_path}"),
            )
        })?;
    if meta.file_type().is_symlink() {
        return Err(HttpError::new(
            StatusCode::FORBIDDEN,
            "symlink_not_allowed",
            "serving symlinked artifacts is not permitted",
        ));
    }
    if !meta.is_file() {
        return Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("artifact not found: {raw_path}"),
        ));
    }

    let bytes = tokio::fs::read(&canonical_candidate).await.map_err(|e| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_error",
            format!("failed to read artifact: {e}"),
        )
    })?;

    let content_type = infer_content_type(&raw_path);
    Ok(([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response())
}

/// Return `true` when `path` contains traversal or absolute-path components
/// that must be rejected before joining with any root.
pub(crate) fn is_structurally_unsafe(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    FsPath::new(path).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

/// Infer a `Content-Type` value from the file extension in `path`.
pub(crate) fn infer_content_type(path: &str) -> &'static str {
    let ext = FsPath::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let ext_lower = ext.to_ascii_lowercase();
    match ext_lower.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "txt" | "log" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
