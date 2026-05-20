use super::super::error::{map_service_error, rest_error};
use super::super::state::RestState;
use crate::services::context::ServiceContext;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn missing_field(field: &'static str) -> Response {
    rest_error(
        StatusCode::BAD_REQUEST,
        "bad_request",
        format!("{field} is required"),
    )
}

pub(super) fn not_found(kind: &'static str, id: Uuid) -> Response {
    rest_error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{kind} job {id} not found"),
    )
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
pub(super) async fn ctx_only(state: &RestState) -> Result<Arc<ServiceContext>, Response> {
    state
        .service_context()
        .await
        .map_err(|err| map_service_error(&*err))
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
pub(super) async fn ctx_and_job_id(
    state: &RestState,
    id: &str,
) -> Result<(Arc<ServiceContext>, Uuid), Response> {
    let job_id = Uuid::parse_str(id).map_err(|_| {
        rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid job id: {id}"),
        )
    })?;
    let ctx = ctx_only(state).await?;
    Ok((ctx, job_id))
}

pub(super) fn cancel_response(canceled: bool) -> Response {
    Json(serde_json::json!({ "canceled": canceled })).into_response()
}

pub(super) fn count_response(action: &'static str, count: u64) -> Response {
    Json(serde_json::json!({ action: count })).into_response()
}

pub(super) fn validate_urls(urls: &[String]) -> Result<(), String> {
    for url in urls {
        crate::core::http::validate_url(url).map_err(|e| format!("{url}: {e}"))?;
    }
    Ok(())
}

pub(super) fn validate_embed_input(input: &str) -> Result<(), String> {
    let input = input.trim();
    if input.starts_with("http://") || input.starts_with("https://") {
        return crate::core::http::validate_url(input).map_err(|e| e.to_string());
    }
    let path = Path::new(input);
    if !path.exists() {
        return Ok(());
    }
    if std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("local embed path must not be a symlink".into());
    }
    let allowed_roots: Vec<std::path::PathBuf> = std::env::var("AXON_MCP_EMBED_ALLOWED_ROOTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|p| {
                    let t = p.trim();
                    (!t.is_empty()).then(|| std::path::PathBuf::from(t))
                })
                .collect()
        })
        .unwrap_or_default();
    if allowed_roots.is_empty() {
        return Err(
            "local file embedding is disabled; set AXON_MCP_EMBED_ALLOWED_ROOTS to allow specific roots".into()
        );
    }
    let canonical = std::fs::canonicalize(path).map_err(|e| format!("invalid embed path: {e}"))?;
    let root = allowed_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| canonical.starts_with(root))
        .ok_or_else(|| {
            format!(
                "local embed path must be under one of AXON_MCP_EMBED_ALLOWED_ROOTS; got: {input}"
            )
        })?;
    let relative = canonical
        .strip_prefix(&root)
        .map_err(|_| "local embed path is outside the allowed root".to_string())?;
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.') {
            return Err("local embed path must not include dotfiles".into());
        }
    }
    validate_no_symlink_children(path)?;
    Ok(())
}

fn validate_no_symlink_children(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("inspect embed path for symlinks: {e}"))?;
    if metadata.file_type().is_symlink() {
        return Err("local embed path must not include symlinks".into());
    }
    if !metadata.is_dir() {
        return Ok(());
    }

    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("read embed directory {:?}: {e}", dir))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("read embed directory entry: {e}"))?;
            let child = entry.path();
            let metadata = std::fs::symlink_metadata(&child)
                .map_err(|e| format!("inspect embed path {:?}: {e}", child))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "local embed path must not include symlinks: {}",
                    child.display()
                ));
            }
            if metadata.is_dir() {
                pending.push(child);
            }
        }
    }
    Ok(())
}
