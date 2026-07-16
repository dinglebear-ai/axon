//! Opaque-ID artifact metadata and content routes.
//!
//! The `/v1` surface never accepts a filesystem path. The path-confined helper
//! at the bottom remains private to the authenticated first-run panel.

use crate::server::error::HttpError;
use axon_api::source::{ArtifactId, ArtifactListRequest};
use axon_services::artifacts::{ArtifactDetail, ArtifactSummary};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use std::{
    path::{Path as StdPath, PathBuf},
    sync::Arc,
};
use tokio_util::io::ReaderStream;

/// `(AppState, Arc<Config>)` state shape used by all exploration/artifact handlers.
type WebState = (
    super::super::state::AppState,
    Arc<axon_core::config::Config>,
);

#[utoipa::path(
    get,
    path = "/v1/artifacts",
    params(
        ("source_id" = Option<String>, Query, description = "Filter by source identifier"),
        ("job_id" = Option<String>, Query, description = "Filter by job identifier"),
        ("kind" = Option<String>, Query, description = "Filter by artifact kind"),
        ("limit" = Option<u32>, Query, description = "Page size, capped at 200"),
        ("cursor" = Option<String>, Query, description = "Opaque artifact keyset cursor")
    ),
    responses(
        (status = 200, description = "Paged artifact metadata", body = axon_api::source::Page<ArtifactSummary>),
        (status = 400, description = "Invalid artifact filter or cursor", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn list_artifacts(
    State((state, _cfg)): State<WebState>,
    Query(request): Query<ArtifactListRequest>,
) -> Result<super::super::json::Json<axon_api::source::Page<ArtifactSummary>>, HttpError> {
    axon_services::artifacts::list_artifacts(&state.service_context, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    get,
    path = "/v1/artifacts/{artifact_id}",
    params(("artifact_id" = String, Path, description = "Opaque artifact identifier")),
    responses(
        (status = 200, description = "Artifact metadata", body = ArtifactDetail),
        (status = 404, description = "Artifact not found", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn get_artifact(
    State((state, _cfg)): State<WebState>,
    Path(artifact_id): Path<ArtifactId>,
) -> Result<super::super::json::Json<ArtifactDetail>, HttpError> {
    axon_services::artifacts::get_artifact(&state.service_context, artifact_id)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub(crate) struct ArtifactContentQuery {
    /// Force download disposition even for browser-safe image content.
    #[serde(default)]
    download: bool,
}

#[utoipa::path(
    get,
    path = "/v1/artifacts/{artifact_id}/content",
    params(
        ("artifact_id" = String, Path, description = "Opaque artifact identifier"),
        ArtifactContentQuery
    ),
    responses(
        (status = 200, description = "Artifact bytes"),
        (status = 404, description = "Artifact not found", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn artifact_content(
    State((state, _cfg)): State<WebState>,
    Path(artifact_id): Path<ArtifactId>,
    Query(query): Query<ArtifactContentQuery>,
) -> Result<Response, HttpError> {
    let content = axon_services::artifacts::artifact_content(&state.service_context, artifact_id)
        .await
        .map_err(HttpError::from_api_error)?;
    let mut response = Body::from(content.bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content.content_type).map_err(|_| {
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_error",
                "artifact has an invalid content type",
            )
        })?,
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if query.download || !is_inline_content_type(&content.content_type) {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&content.disposition).map_err(|_| {
                HttpError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "read_error",
                    "artifact has an invalid content disposition",
                )
            })?,
        );
    }
    Ok(response)
}

/// Private first-run panel helper. Public REST callers use opaque artifact IDs.
pub(crate) async fn serve_artifact_from_path(
    cfg: &axon_core::config::Config,
    raw_path: String,
) -> Result<Response, HttpError> {
    let artifact_path = resolve_artifact_path(&cfg.output_dir, &raw_path).await?;
    let file = tokio::fs::File::open(&artifact_path)
        .await
        .map_err(|e| open_artifact_error(&e, &raw_path))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    Ok(artifact_headers_for_path(&raw_path).into_response(body))
}

/// Map a `File::open` failure for an already-validated artifact to an HTTP error.
///
/// `resolve_artifact_path` validated existence, so a `NotFound` here means the
/// file vanished in the TOCTOU window — a 404, not a server error. Every other
/// IO error (permission denied, etc.) is a genuine 500.
fn open_artifact_error(error: &std::io::Error, raw_path: &str) -> HttpError {
    if error.kind() == std::io::ErrorKind::NotFound {
        artifact_not_found(raw_path)
    } else {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_error",
            format!("failed to open artifact: {error}"),
        )
    }
}

pub(crate) async fn resolve_artifact_path(
    root: &StdPath,
    raw_path: &str,
) -> Result<PathBuf, HttpError> {
    if is_structurally_unsafe(raw_path) {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "path contains traversal components or is absolute",
        ));
    }

    let canonical_root = tokio::fs::canonicalize(root).await.map_err(|_| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "output_dir_error",
            "output directory is not accessible",
        )
    })?;
    reject_symlink_components(&canonical_root, raw_path).await?;

    let candidate = root.join(raw_path);
    let canonical_candidate = tokio::fs::canonicalize(&candidate).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            artifact_not_found(raw_path)
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

    let meta = tokio::fs::metadata(&canonical_candidate)
        .await
        .map_err(|_| artifact_not_found(raw_path))?;
    if !meta.is_file() {
        return Err(artifact_not_found(raw_path));
    }
    Ok(canonical_candidate)
}

fn artifact_not_found(path: &str) -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("artifact not found: {path}"),
    )
}

async fn reject_symlink_components(root: &StdPath, raw_path: &str) -> Result<(), HttpError> {
    let mut current: PathBuf = root.to_path_buf();
    // Defense-in-depth only: decode so that an encoded separator/component can't
    // hide a symlink hop from this walk. The authoritative escape guard is the
    // `canonicalize()` + `starts_with(canonical_root)` check in
    // `resolve_artifact_path`, which operates on the joined raw path.
    let decoded = percent_decode_str(raw_path).decode_utf8_lossy();
    for component in decoded.split('/') {
        if component.is_empty() {
            continue;
        }
        current.push(component);
        if let Ok(meta) = tokio::fs::symlink_metadata(&current).await
            && meta.file_type().is_symlink()
        {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "symlink_not_allowed",
                "serving symlinked artifacts is not permitted",
            ));
        }
    }
    Ok(())
}

/// Return `true` when `path` contains traversal or absolute-path components
/// that must be rejected before joining with any root.
pub(crate) fn is_structurally_unsafe(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\0') || path.contains('\\') {
        return true;
    }
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    if decoded.contains(':')
        || decoded.contains('\\')
        || decoded
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return true;
    }
    StdPath::new(decoded.as_ref()).components().any(|c| {
        matches!(
            c,
            std::path::Component::CurDir
                | std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactHeaders {
    pub(crate) content_type: &'static str,
    pub(crate) content_disposition: Option<String>,
}

impl ArtifactHeaders {
    fn into_response(self, body: Body) -> Response {
        let mut response = body.into_response();
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(self.content_type),
        );
        headers.insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
        if let Some(disposition) = self.content_disposition {
            let value =
                HeaderValue::from_str(&disposition).expect("artifact disposition is ASCII-safe");
            headers.insert(header::CONTENT_DISPOSITION, value);
        }
        response
    }
}

pub(crate) fn artifact_headers_for_path(path: &str) -> ArtifactHeaders {
    let ext = StdPath::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let ext_lower = ext.to_ascii_lowercase();
    let content_type = match ext_lower.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "json" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "txt" | "log" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    let content_disposition = (!is_inline_content_type(content_type))
        .then(|| format!("attachment; filename=\"{}\"", safe_download_filename(path)));
    ArtifactHeaders {
        content_type,
        content_disposition,
    }
}

fn is_inline_content_type(content_type: &str) -> bool {
    matches!(
        content_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/avif"
    )
}

fn safe_download_filename(path: &str) -> String {
    let decoded = percent_decode_str(path)
        .decode_utf8_lossy()
        .replace('\\', "/");
    let leaf = decoded.rsplit('/').next().unwrap_or("artifact");
    let sanitized: String = leaf
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '\r' | '\n' | '\0' => '_',
            ch if ch.is_ascii_graphic() || ch == ' ' => ch,
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "artifact".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
