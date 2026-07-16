use super::{MAX_UPLOAD_BYTES, upload_error};
use axon_api::source::{ApiError, MetadataMap, UploadCreateRequest, UploadId};
use axon_core::redact::{
    DefaultRedactor, RedactionContext, Redactor, redact_metadata_checked, stamp_redaction_metadata,
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn validate_create(request: &UploadCreateRequest) -> Result<(), ApiError> {
    let filename = Path::new(&request.filename);
    if request.filename.is_empty()
        || request.filename.len() > 255
        || filename.file_name().and_then(|value| value.to_str()) != Some(request.filename.as_str())
        || request.filename.contains(['/', '\\', '\0'])
    {
        return Err(upload_error(
            "upload.filename_invalid",
            "filename must be one safe basename",
        ));
    }
    if request.content_type.is_empty()
        || request.content_type.len() > 255
        || request.content_type.contains(['\r', '\n', '\0'])
    {
        return Err(upload_error(
            "upload.content_type_invalid",
            "content_type is invalid",
        ));
    }
    if request.size_bytes > MAX_UPLOAD_BYTES {
        return Err(upload_error(
            "upload.too_large",
            "upload exceeds the server size limit",
        ));
    }
    if let Some(hash) = request.sha256.clone() {
        normalize_sha256(hash)?;
    }
    Ok(())
}

pub(super) fn validate_upload_id(upload_id: &UploadId) -> Result<(), ApiError> {
    let value = &upload_id.0;
    if value.starts_with("upl_")
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(upload_error(
            "upload.id_invalid",
            "upload_id must be an opaque upl_ identifier",
        ))
    }
}

pub(super) fn validate_artifact_id(value: &str) -> Result<(), ApiError> {
    if value.starts_with("art_")
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(upload_error(
            "artifact.invalid_id",
            "artifact_id must be an opaque art_ identifier",
        ))
    }
}

pub(super) fn normalize_sha256(value: String) -> Result<String, ApiError> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(upload_error(
            "upload.sha256_invalid",
            "sha256 must be 64 hexadecimal characters",
        ))
    }
}

pub(super) fn verify_hash(expected: Option<&str>, actual: &str) -> Result<(), ApiError> {
    if expected.is_some_and(|expected| expected != actual) {
        Err(upload_error(
            "upload.sha256_mismatch",
            "upload sha256 does not match",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn normalized_content_type(value: &str) -> &str {
    value.split(';').next().unwrap_or_default().trim()
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn redact_upload_metadata(metadata: MetadataMap) -> Result<MetadataMap, ApiError> {
    let context = RedactionContext::artifact_metadata();
    let (metadata, report) = redact_metadata_checked(metadata, &context, &DefaultRedactor::new())?;
    Ok(stamp_redaction_metadata(metadata, &report))
}

pub(super) fn bounded_reason(reason: &str) -> String {
    let bounded: String = reason.chars().take(512).collect();
    DefaultRedactor::new().redact_text(&bounded, &RedactionContext::artifact_metadata())
}
