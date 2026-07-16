use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use base64::Engine as _;

use super::{UploadSourceAdapter, validate_adapter};
use crate::adapter::Result;

#[async_trait]
pub trait UploadSourceProvider: Send + Sync {
    async fn get(&self, upload_id: &str) -> Result<Option<ArtifactReadResult>>;
}

impl UploadSourceAdapter {
    pub async fn materialize(
        &self,
        mut plan: SourcePlan,
        provider: Arc<dyn UploadSourceProvider>,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_adapter(&plan)?;
        let upload_id = upload_id_from_uri(&plan.route.source.canonical_uri)?;
        let staged = provider
            .get(upload_id)
            .await?
            .ok_or_else(|| missing_upload(upload_id))?;
        let content = staged.content.ok_or_else(|| {
            ApiError::new(
                "adapter.upload.content_missing",
                axon_error::ErrorStage::Fetching,
                "staged upload has no readable content",
            )
            .with_context("upload_id", upload_id.to_string())
        })?;
        let temporary = tempfile::tempdir().map_err(|error| {
            ApiError::new(
                "adapter.upload.materialize_failed",
                axon_error::ErrorStage::Fetching,
                error.to_string(),
            )
        })?;
        let path = temporary.path().join(staged_filename(
            &staged.metadata,
            &staged.content_type,
            upload_id,
        ));
        tokio::fs::write(&path, upload_bytes(content, upload_id)?)
            .await
            .map_err(|error| {
                ApiError::new(
                    "adapter.upload.materialize_failed",
                    axon_error::ErrorStage::Fetching,
                    error.to_string(),
                )
                .with_context("upload_id", upload_id.to_string())
            })?;
        plan.request.source = path.to_string_lossy().to_string();
        plan.request.scope = Some(SourceScope::File);
        plan.route.scope = SourceScope::File;
        Ok(crate::acquisition::MaterializedSource::temporary_at(
            plan, temporary, path,
        ))
    }
}

pub fn upload_id_from_uri(uri: &str) -> Result<&str> {
    let upload_id = uri
        .strip_prefix("upload://")
        .or_else(|| uri.strip_prefix("artifact://"))
        .ok_or_else(|| invalid_upload_uri(uri))?;
    if upload_id.is_empty()
        || upload_id.len() > 200
        || (!upload_id.starts_with("upl_") && !upload_id.starts_with("art_"))
        || upload_id
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid_upload_uri(uri));
    }
    Ok(upload_id)
}

fn upload_bytes(content: ContentRef, upload_id: &str) -> Result<Vec<u8>> {
    match content {
        ContentRef::InlineText { text } => Ok(text.into_bytes()),
        ContentRef::InlineBytes { bytes_base64, .. } => base64::engine::general_purpose::STANDARD
            .decode(bytes_base64)
            .map_err(|error| {
                ApiError::new(
                    "adapter.upload.content_invalid",
                    axon_error::ErrorStage::Fetching,
                    error.to_string(),
                )
                .with_context("upload_id", upload_id.to_string())
            }),
        ContentRef::Artifact { .. } | ContentRef::External { .. } => Err(ApiError::new(
            "adapter.upload.content_unresolved",
            axon_error::ErrorStage::Fetching,
            "staged upload content must be resolved before adapter materialization",
        )
        .with_context("upload_id", upload_id.to_string())),
    }
}

fn staged_filename(metadata: &MetadataMap, content_type: &str, upload_id: &str) -> String {
    metadata
        .get("filename")
        .or_else(|| metadata.get("upload_filename"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{upload_id}.{}", extension_for(content_type)))
}

fn extension_for(content_type: &str) -> &'static str {
    match content_type.split(';').next().unwrap_or_default().trim() {
        "text/markdown" => "md",
        "application/json" => "json",
        "text/html" => "html",
        "text/plain" => "txt",
        _ => "bin",
    }
}

fn invalid_upload_uri(uri: &str) -> ApiError {
    ApiError::new(
        "adapter.upload.identity_invalid",
        axon_error::ErrorStage::Resolving,
        "upload source must be exactly upload://upl_<id> or artifact://art_<id>",
    )
    .with_context("canonical_uri", uri.to_string())
}

fn missing_upload(upload_id: &str) -> ApiError {
    ApiError::new(
        "adapter.upload.not_found",
        axon_error::ErrorStage::Fetching,
        "staged upload identity does not exist",
    )
    .with_context("upload_id", upload_id.to_string())
}
