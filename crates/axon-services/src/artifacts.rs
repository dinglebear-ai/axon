//! Typed artifact metadata and content service.
//!
//! Public callers address artifacts only by opaque [`ArtifactId`]. Filesystem
//! paths remain an implementation detail of the configured `ArtifactStore`.

use crate::context::ServiceContext;
use axon_api::source::{
    ApiError, ArtifactHandle, ArtifactId, ArtifactKind, ArtifactListRequest, ContentRef,
    ErrorStage, JobId, MetadataMap, Page, SourceId, Timestamp,
};
use axon_core::boundary::{ArtifactStore, FileArtifactStore};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

// Reset still owns these internal filesystem operations. Keeping them
// crate-private avoids recreating the former public wildcard facade.
pub(crate) use axon_core::artifacts::{artifact_root, count_files, purge_files};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ArtifactSummary {
    pub artifact_id: ArtifactId,
    pub kind: ArtifactKind,
    pub created_at: Timestamp,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ArtifactDetail {
    #[serde(flatten)]
    pub summary: ArtifactSummary,
    pub retention: serde_json::Value,
    pub producer_refs: Vec<String>,
    pub content_url: String,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ArtifactContentDescriptor {
    pub artifact_id: ArtifactId,
    pub content_type: String,
    pub disposition: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredArtifactManifest {
    handle: ArtifactHandle,
    content_type: String,
    content_path: String,
    metadata: MetadataMap,
}

pub async fn list_artifacts(
    ctx: &ServiceContext,
    request: ArtifactListRequest,
) -> Result<Page<ArtifactSummary>, ApiError> {
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    if let Some(cursor) = request.cursor.as_deref() {
        validate_artifact_id(cursor)?;
    }

    let mut manifests = read_manifests(&artifact_root_for(ctx)).await?;
    manifests.sort_by(|left, right| left.handle.artifact_id.cmp(&right.handle.artifact_id));
    let mut matching = Vec::new();
    for manifest in manifests {
        let artifact_id = &manifest.handle.artifact_id.0;
        if request
            .cursor
            .as_deref()
            .is_some_and(|cursor| artifact_id.as_str() <= cursor)
        {
            continue;
        }
        let summary = manifest_summary(ctx, &manifest).await?;
        if request.kind.is_some_and(|kind| summary.kind != kind)
            || request
                .source_id
                .as_ref()
                .is_some_and(|id| summary.source_id.as_ref() != Some(id))
            || request
                .job_id
                .as_ref()
                .is_some_and(|id| summary.job_id.as_ref() != Some(id))
        {
            continue;
        }
        matching.push(summary);
        if matching.len() > limit as usize {
            break;
        }
    }
    let has_more = matching.len() > limit as usize;
    matching.truncate(limit as usize);
    let next_cursor = has_more.then(|| {
        matching
            .last()
            .expect("non-empty page")
            .artifact_id
            .0
            .clone()
    });
    Ok(Page {
        items: matching,
        next_cursor,
        limit,
        total: None,
    })
}

pub async fn get_artifact(
    ctx: &ServiceContext,
    artifact_id: ArtifactId,
) -> Result<ArtifactDetail, ApiError> {
    let manifest = read_manifest(ctx, &artifact_id).await?;
    let summary = manifest_summary(ctx, &manifest).await?;
    let retention = manifest
        .metadata
        .get("retention")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let producer_refs = metadata_string_list(&manifest.metadata, "producer_refs");
    Ok(ArtifactDetail {
        content_url: format!("/v1/artifacts/{}/content", artifact_id.0),
        summary,
        retention,
        producer_refs,
        metadata: manifest.metadata,
    })
}

pub async fn artifact_content(
    ctx: &ServiceContext,
    artifact_id: ArtifactId,
) -> Result<ArtifactContentDescriptor, ApiError> {
    let manifest = read_manifest(ctx, &artifact_id).await?;
    let store = FileArtifactStore::new(artifact_root_for(ctx));
    let result = store.get(manifest.handle).await?;
    let bytes = content_bytes(result.content)?;
    let label = metadata_string(&result.metadata, "label")
        .unwrap_or_else(|| format!("{}.bin", artifact_id.0));
    Ok(ArtifactContentDescriptor {
        artifact_id,
        content_type: result.content_type,
        disposition: safe_disposition(&label),
        bytes,
    })
}

async fn read_manifests(root: &Path) -> Result<Vec<StoredArtifactManifest>, ApiError> {
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error("artifact.list_failed", root, error)),
    };
    let mut manifests = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| io_error("artifact.list_failed", root, error))?
    {
        let path = entry.path();
        let is_manifest = path.extension().and_then(|value| value.to_str()) == Some("json")
            && path
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("artifact_"));
        if !is_manifest {
            continue;
        }
        let manifest = parse_manifest(&path).await?;
        validate_manifest(&manifest)?;
        manifests.push(manifest);
    }
    Ok(manifests)
}

async fn read_manifest(
    ctx: &ServiceContext,
    artifact_id: &ArtifactId,
) -> Result<StoredArtifactManifest, ApiError> {
    validate_artifact_id(&artifact_id.0)?;
    let path = artifact_root_for(ctx).join(format!("{}.json", artifact_id.0));
    let manifest = parse_manifest(&path).await?;
    validate_manifest(&manifest)?;
    if manifest.handle.artifact_id != *artifact_id {
        return Err(artifact_error(
            "artifact.invalid_manifest",
            "artifact manifest identity does not match the requested artifact",
        ));
    }
    Ok(manifest)
}

async fn parse_manifest(path: &Path) -> Result<StoredArtifactManifest, ApiError> {
    let bytes = tokio::fs::read(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            artifact_error("artifact.not_found", "artifact not found")
        } else {
            io_error("artifact.read_failed", path, error)
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        artifact_error(
            "artifact.invalid_manifest",
            format!("failed to parse artifact manifest: {error}"),
        )
    })
}

async fn manifest_summary(
    ctx: &ServiceContext,
    manifest: &StoredArtifactManifest,
) -> Result<ArtifactSummary, ApiError> {
    let content_path = artifact_root_for(ctx).join(&manifest.content_path);
    let metadata = tokio::fs::metadata(&content_path)
        .await
        .map_err(|error| io_error("artifact.read_failed", &content_path, error))?;
    let created_at = metadata
        .created()
        .or_else(|_| metadata.modified())
        .map(chrono::DateTime::<chrono::Utc>::from)
        .map(Timestamp::from)
        .unwrap_or_else(|_| Timestamp("1970-01-01T00:00:00Z".to_string()));
    Ok(ArtifactSummary {
        artifact_id: manifest.handle.artifact_id.clone(),
        kind: manifest.handle.artifact_kind,
        created_at,
        size_bytes: metadata.len(),
        source_id: metadata_string(&manifest.metadata, "source_id").map(SourceId::new),
        job_id: metadata_string(&manifest.metadata, "job_id")
            .and_then(|value| value.parse().ok())
            .map(JobId::new),
        content_type: Some(manifest.content_type.clone()),
        label: metadata_string(&manifest.metadata, "label"),
    })
}

fn content_bytes(content: Option<ContentRef>) -> Result<Vec<u8>, ApiError> {
    match content {
        Some(ContentRef::InlineText { text }) => Ok(text.into_bytes()),
        Some(ContentRef::InlineBytes { bytes_base64, .. }) => {
            base64::engine::general_purpose::STANDARD
                .decode(bytes_base64)
                .map_err(|error| artifact_error("artifact.invalid_content", error.to_string()))
        }
        _ => Err(artifact_error(
            "artifact.content_unavailable",
            "artifact content is not available as stored bytes",
        )),
    }
}

fn validate_artifact_id(value: &str) -> Result<(), ApiError> {
    let valid = value.starts_with("artifact_")
        && value.len() <= 160
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if valid {
        Ok(())
    } else {
        Err(artifact_error(
            "artifact.invalid_id",
            "artifact_id must be an opaque artifact_ identifier",
        ))
    }
}

fn validate_manifest(manifest: &StoredArtifactManifest) -> Result<(), ApiError> {
    validate_artifact_id(&manifest.handle.artifact_id.0)?;
    let expected = format!("{}.bin", manifest.handle.artifact_id.0);
    if manifest.content_path != expected
        || Path::new(&manifest.content_path)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(manifest.content_path.as_str())
    {
        return Err(artifact_error(
            "artifact.invalid_manifest",
            "artifact manifest content path does not match its opaque identifier",
        ));
    }
    Ok(())
}

fn artifact_root_for(ctx: &ServiceContext) -> PathBuf {
    ctx.cfg.output_dir.join("artifacts")
}

fn metadata_string(metadata: &MetadataMap, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn metadata_string_list(metadata: &MetadataMap, key: &str) -> Vec<String> {
    metadata
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn safe_disposition(label: &str) -> String {
    let filename: String = label
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '\r' | '\n' | '\0' => '_',
            ch if ch.is_ascii_graphic() || ch == ' ' => ch,
            _ => '_',
        })
        .collect();
    format!(
        "attachment; filename=\"{}\"",
        if filename.is_empty() {
            "artifact"
        } else {
            &filename
        }
    )
}

fn artifact_error(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Retrieving, message)
}

fn io_error(code: &str, path: &Path, error: std::io::Error) -> ApiError {
    artifact_error(
        code,
        format!("artifact IO failed at {}: {error}", path.display()),
    )
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
