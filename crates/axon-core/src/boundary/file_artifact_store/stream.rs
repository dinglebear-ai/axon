//! Sealed archive persistence and its cross-process settlement fence.
use super::*;

pub(super) fn artifact_io_error(error: std::io::Error) -> ApiError {
    ApiError::new(
        "artifact.write_failed",
        ErrorStage::Publishing,
        error.to_string(),
    )
}

pub(super) struct ArtifactDiskOwner(std::fs::File);
impl Drop for ArtifactDiskOwner {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
#[allow(clippy::result_large_err)] // Preserve the ArtifactStore structured ApiError contract.
pub(super) fn artifact_disk_owner(root: &Path, nonblocking: bool) -> Result<ArtifactDiskOwner> {
    std::fs::create_dir_all(root).map_err(artifact_io_error)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(root.join(".archive-write.lock"))
        .map_err(artifact_io_error)?;
    if !file.metadata().map_err(artifact_io_error)?.is_file() {
        return Err(ApiError::new(
            "artifact.invalid_owner",
            ErrorStage::Publishing,
            "archive settlement fence must be a regular file",
        ));
    }
    if nonblocking {
        file.try_lock().map_err(|error| {
            ApiError::new(
                "artifact.writer_busy",
                ErrorStage::Cleaning,
                format!("archive writer still owns its settlement fence: {error}"),
            )
        })?;
    } else {
        file.lock().map_err(artifact_io_error)?;
    }
    Ok(ArtifactDiskOwner(file))
}

#[allow(clippy::result_large_err)] // Preserve the ArtifactStore structured ApiError contract.
pub(super) fn file_identity(artifact: &ArtifactFileWriteRequest) -> Result<ArtifactHandle> {
    let digest = artifact
        .content_hash
        .strip_prefix("sha256:")
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            ApiError::new(
                "artifact.integrity_failed",
                ErrorStage::Publishing,
                "invalid sealed artifact digest",
            )
        })?;
    let metadata = redact_artifact_metadata(artifact.metadata.clone())?;
    let identity = artifact_identity_digest_parts(
        artifact.kind,
        artifact.source_id.as_ref(),
        artifact.job_id.as_ref(),
        &metadata,
        digest,
    )
    .map_err(|error| *error)?;
    let id = ArtifactId::new(format!(
        "art_{}_{}",
        artifact_kind_slug(artifact.kind),
        &identity[..16]
    ));
    Ok(ArtifactHandle {
        artifact_id: id.clone(),
        artifact_kind: artifact.kind,
        uri: Some(format!("artifact://{}", id.0)),
    })
}

#[allow(clippy::result_large_err)] // Preserve the ArtifactStore structured ApiError contract.
pub(super) fn put_sealed_file(
    root: PathBuf,
    mut artifact: ArtifactFileWriteRequest,
) -> Result<ArtifactHandle> {
    use sha2::{Digest as _, Sha256};
    use std::io::{Read as _, Seek as _, Write as _};
    let failed = |error: std::io::Error| {
        ApiError::new(
            "artifact.write_failed",
            ErrorStage::Publishing,
            error.to_string(),
        )
    };
    if artifact.size_bytes > 256 * 1024 * 1024 {
        return Err(ApiError::new(
            "artifact.byte_limit",
            ErrorStage::Publishing,
            "streamed artifact exceeds 256 MiB admission",
        ));
    }
    let handle = file_identity(&artifact)?;
    let metadata = redact_artifact_metadata(artifact.metadata)?;
    std::fs::create_dir_all(&root).map_err(failed)?;
    let mut content = tempfile::Builder::new()
        .prefix(&format!(
            ".{}.stream-",
            safe_artifact_id(&handle.artifact_id)
        ))
        .tempfile_in(&root)
        .map_err(failed)?;
    artifact.file.rewind().map_err(failed)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = artifact.file.read(&mut buffer).map_err(failed)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| failed(std::io::Error::other("artifact length overflow")))?;
        if total > artifact.size_bytes {
            return Err(ApiError::new(
                "artifact.integrity_failed",
                ErrorStage::Publishing,
                "artifact exceeds sealed length",
            ));
        }
        content.write_all(&buffer[..count]).map_err(failed)?;
        digest.update(&buffer[..count]);
    }
    let digest = hex::encode(digest.finalize());
    if total != artifact.size_bytes || artifact.content_hash != format!("sha256:{digest}") {
        return Err(ApiError::new(
            "artifact.integrity_failed",
            ErrorStage::Publishing,
            "artifact stream differs from sealed receipt",
        ));
    }
    let id = handle.artifact_id.clone();
    let content_name = format!("{}.bin", safe_artifact_id(&id));
    let manifest = FileArtifactManifest {
        handle: handle.clone(),
        content_type: artifact.content_type,
        content_path: content_name.clone(),
        content_kind: "inline_bytes".into(),
        metadata,
    };
    let mut manifest_file = tempfile::Builder::new()
        .prefix(&format!(".{}.manifest-", safe_artifact_id(&id)))
        .tempfile_in(&root)
        .map_err(failed)?;
    serde_json::to_writer_pretty(&mut manifest_file, &manifest).map_err(|error| {
        ApiError::new(
            "artifact.write_failed",
            ErrorStage::Publishing,
            error.to_string(),
        )
    })?;
    content.as_file().sync_all().map_err(failed)?;
    manifest_file.as_file().sync_all().map_err(failed)?;
    // Content is complete before its discoverable manifest is published.
    content
        .persist(root.join(content_name))
        .map_err(|error| failed(error.error))?;
    manifest_file
        .persist(root.join(format!("{}.json", safe_artifact_id(&id))))
        .map_err(|error| failed(error.error))?;
    std::fs::File::open(&root)
        .and_then(|directory| directory.sync_all())
        .map_err(failed)?;
    Ok(handle)
}

#[cfg(test)]
mod stream_settlement_tests {
    use super::*;
    #[tokio::test]
    async fn cleanup_preserves_an_active_writer_and_removes_only_its_owned_crash_temporary_files() {
        let root = tempfile::tempdir().unwrap();
        let store = FileArtifactStore::new(root.path());
        let handle = ArtifactHandle {
            artifact_id: ArtifactId::new("art_warc_fixture"),
            artifact_kind: ArtifactKind::Warc,
            uri: None,
        };
        let orphan = root.path().join(".art_warc_fixture.stream-orphan");
        let manifest_orphan = root.path().join(".art_warc_fixture.manifest-orphan");
        let unrelated = root.path().join(".art_warc_other.stream-orphan");
        std::fs::write(&orphan, b"partial").unwrap();
        std::fs::write(&manifest_orphan, b"partial manifest").unwrap();
        std::fs::write(&unrelated, b"other attempt").unwrap();
        let owner = artifact_disk_owner(root.path(), false).unwrap();
        let error = store.delete(handle.clone()).await.unwrap_err();
        assert_eq!(error.code.0, "artifact.writer_busy");
        assert!(orphan.exists());
        drop(owner);
        store.delete(handle).await.unwrap();
        assert!(!orphan.exists());
        assert!(!manifest_orphan.exists());
        assert!(unrelated.exists());
    }
}
