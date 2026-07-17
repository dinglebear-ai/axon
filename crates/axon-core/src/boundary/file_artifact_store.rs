use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axon_api::source::*;
use base64::Engine as _;

use super::{
    ArtifactBytesWriteRequest, ArtifactStore, Result, capability, redact_artifact_metadata,
};

#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    root: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FileArtifactManifest {
    handle: ArtifactHandle,
    content_type: String,
    content_path: String,
    content_kind: String,
    metadata: MetadataMap,
}

struct FileArtifactWrite {
    kind: ArtifactKind,
    content_type: String,
    content_kind: &'static str,
    source_id: Option<SourceId>,
    job_id: Option<JobId>,
    metadata: MetadataMap,
    bytes: Vec<u8>,
}

impl FileArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn content_path(&self, artifact_id: &ArtifactId) -> PathBuf {
        self.root
            .join(format!("{}.bin", safe_artifact_id(artifact_id)))
    }

    fn manifest_path(&self, artifact_id: &ArtifactId) -> PathBuf {
        self.root
            .join(format!("{}.json", safe_artifact_id(artifact_id)))
    }

    async fn put_content_bytes(&self, write: FileArtifactWrite) -> Result<ArtifactHandle> {
        let metadata = redact_artifact_metadata(write.metadata)?;
        let digest = sha256_hex(&write.bytes);
        let identity_digest = artifact_identity_digest_parts(
            write.kind,
            write.source_id.as_ref(),
            write.job_id.as_ref(),
            &metadata,
            &digest,
        )
        .map_err(|error| *error)?;
        let artifact_id = ArtifactId::new(format!(
            "art_{}_{}",
            artifact_kind_slug(write.kind),
            &identity_digest[..16]
        ));
        let content_path = self.content_path(&artifact_id);
        let manifest_path = self.manifest_path(&artifact_id);
        tokio::fs::create_dir_all(&self.root).await.map_err(|err| {
            ApiError::new(
                "artifact.write_failed",
                ErrorStage::Publishing,
                format!(
                    "failed to create artifact directory {}: {err}",
                    self.root.display()
                ),
            )
        })?;
        tokio::fs::write(&content_path, &write.bytes)
            .await
            .map_err(|err| {
                ApiError::new(
                    "artifact.write_failed",
                    ErrorStage::Publishing,
                    format!("failed to write artifact {}: {err}", content_path.display()),
                )
            })?;
        let handle = ArtifactHandle {
            artifact_id: artifact_id.clone(),
            artifact_kind: write.kind,
            uri: Some(format!("file://{}", content_path.display())),
        };
        let manifest = FileArtifactManifest {
            handle: handle.clone(),
            content_type: write.content_type,
            content_path: content_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            content_kind: write.content_kind.to_string(),
            metadata,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|err| {
            ApiError::new(
                "artifact.write_failed",
                ErrorStage::Publishing,
                format!("failed to serialize artifact manifest: {err}"),
            )
        })?;
        tokio::fs::write(&manifest_path, manifest_bytes)
            .await
            .map_err(|err| {
                ApiError::new(
                    "artifact.write_failed",
                    ErrorStage::Publishing,
                    format!(
                        "failed to write artifact manifest {}: {err}",
                        manifest_path.display()
                    ),
                )
            })?;
        Ok(handle)
    }
}

#[async_trait]
impl ArtifactStore for FileArtifactStore {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle> {
        let bytes = content_ref_bytes(&artifact.content).map_err(|error| *error)?;
        self.put_content_bytes(FileArtifactWrite {
            kind: artifact.kind,
            content_type: artifact.content_type,
            content_kind: content_kind(&artifact.content),
            source_id: artifact.source_id,
            job_id: artifact.job_id,
            metadata: artifact.metadata,
            bytes,
        })
        .await
    }

    async fn put_bytes(&self, artifact: ArtifactBytesWriteRequest) -> Result<ArtifactHandle> {
        self.put_content_bytes(FileArtifactWrite {
            kind: artifact.kind,
            content_type: artifact.content_type,
            content_kind: "inline_bytes",
            source_id: artifact.source_id,
            job_id: artifact.job_id,
            metadata: artifact.metadata,
            bytes: artifact.bytes,
        })
        .await
    }

    async fn get(&self, handle: ArtifactHandle) -> Result<ArtifactReadResult> {
        let manifest_path = self.manifest_path(&handle.artifact_id);
        let manifest_bytes = tokio::fs::read(&manifest_path).await.map_err(|err| {
            ApiError::new(
                "artifact.not_found",
                ErrorStage::Retrieving,
                format!(
                    "failed to read artifact manifest {}: {err}",
                    manifest_path.display()
                ),
            )
        })?;
        let manifest: FileArtifactManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|err| {
                ApiError::new(
                    "artifact.read_failed",
                    ErrorStage::Retrieving,
                    format!("failed to parse artifact manifest: {err}"),
                )
            })?;
        let expected_content_path = self
            .content_path(&handle.artifact_id)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        if manifest.handle.artifact_id != handle.artifact_id
            || manifest.content_path != expected_content_path
        {
            return Err(ApiError::new(
                "artifact.read_failed",
                ErrorStage::Retrieving,
                "artifact manifest identity or content path is invalid",
            ));
        }
        let content_path = self.root.join(&manifest.content_path);
        let bytes = tokio::fs::read(&content_path).await.map_err(|err| {
            ApiError::new(
                "artifact.read_failed",
                ErrorStage::Retrieving,
                format!("failed to read artifact {}: {err}", content_path.display()),
            )
        })?;
        let content = match manifest.content_kind.as_str() {
            "inline_text" => Some(ContentRef::InlineText {
                text: String::from_utf8(bytes).map_err(|err| {
                    ApiError::new(
                        "artifact.read_failed",
                        ErrorStage::Retrieving,
                        format!("stored text artifact is not UTF-8: {err}"),
                    )
                })?,
            }),
            "inline_bytes" => Some(ContentRef::InlineBytes {
                bytes_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                mime_type: manifest.content_type.clone(),
            }),
            _ => None,
        };
        Ok(ArtifactReadResult {
            handle: manifest.handle,
            content_type: manifest.content_type,
            content,
            metadata: manifest.metadata,
        })
    }

    async fn delete(&self, handle: ArtifactHandle) -> Result<()> {
        remove_file_if_exists(self.content_path(&handle.artifact_id)).await?;
        remove_file_if_exists(self.manifest_path(&handle.artifact_id)).await?;
        Ok(())
    }

    async fn reset(&self) -> Result<()> {
        match tokio::fs::remove_dir_all(&self.root).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(ApiError::new(
                "artifact.reset_failed",
                ErrorStage::Cleaning,
                format!(
                    "failed to reset artifact directory {}: {err}",
                    self.root.display()
                ),
            )),
        }
    }

    async fn capabilities(&self) -> Result<ArtifactStoreCapability> {
        Ok(capability("file-artifact", "axon-core", HealthStatus::Healthy).into())
    }
}

async fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ApiError::new(
            "artifact.delete_failed",
            ErrorStage::Cleaning,
            format!("failed to delete artifact file {}: {err}", path.display()),
        )),
    }
}

fn content_ref_bytes(content: &ContentRef) -> std::result::Result<Vec<u8>, Box<ApiError>> {
    match content {
        ContentRef::InlineText { text } => Ok(text.as_bytes().to_vec()),
        ContentRef::InlineBytes { bytes_base64, .. } => base64::engine::general_purpose::STANDARD
            .decode(bytes_base64)
            .map_err(|err| {
                Box::new(ApiError::new(
                    "artifact.invalid_content",
                    ErrorStage::Publishing,
                    format!("inline bytes artifact content is not valid base64: {err}"),
                ))
            }),
        ContentRef::Artifact { artifact_id } => Ok(artifact_id.0.as_bytes().to_vec()),
        ContentRef::External { uri, integrity } => Ok(integrity
            .as_deref()
            .unwrap_or(uri.as_str())
            .as_bytes()
            .to_vec()),
    }
}

fn content_kind(content: &ContentRef) -> &'static str {
    match content {
        ContentRef::InlineText { .. } => "inline_text",
        ContentRef::InlineBytes { .. } => "inline_bytes",
        ContentRef::Artifact { .. } => "artifact_ref",
        ContentRef::External { .. } => "external_ref",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn artifact_identity_digest_parts(
    kind: ArtifactKind,
    source_id: Option<&SourceId>,
    job_id: Option<&JobId>,
    metadata: &MetadataMap,
    content_digest: &str,
) -> std::result::Result<String, Box<ApiError>> {
    let mut identity = serde_json::Map::new();
    identity.insert(
        "kind".to_string(),
        serde_json::to_value(kind).map_err(|error| Box::new(identity_json_error(error)))?,
    );
    identity.insert(
        "content_digest".to_string(),
        serde_json::json!(content_digest),
    );
    identity.insert(
        "source_id".to_string(),
        serde_json::json!(source_id.map(|value| value.0.clone())),
    );
    identity.insert(
        "job_id".to_string(),
        serde_json::json!(job_id.map(|value| value.0.to_string())),
    );
    identity.insert("metadata".to_string(), serde_json::json!(metadata));
    let bytes =
        serde_json::to_vec(&identity).map_err(|error| Box::new(identity_json_error(error)))?;
    Ok(sha256_hex(&bytes))
}

fn identity_json_error(err: serde_json::Error) -> ApiError {
    ApiError::new(
        "artifact.identity_failed",
        ErrorStage::Publishing,
        format!("failed to build artifact identity: {err}"),
    )
}

fn safe_artifact_id(artifact_id: &ArtifactId) -> String {
    artifact_id
        .0
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn artifact_kind_slug(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::RawContent => "raw",
        ArtifactKind::NormalizedContent => "normalized",
        ArtifactKind::Manifest => "manifest",
        ArtifactKind::Report => "report",
        ArtifactKind::Screenshot => "screenshot",
        ArtifactKind::Warc => "warc",
        ArtifactKind::ProviderTrace => "provider_trace",
    }
}
