use super::{flag_value, flag_values, parse_u32_flag, positional, print_value};
use axon_api::source::{
    ArtifactId, ArtifactKind, ArtifactListRequest, JobId, MetadataMap, SourceId,
    UploadAbortRequest, UploadCompleteRequest, UploadCreateRequest, UploadId, UploadListRequest,
    UploadPurpose, UploadStatusKind,
};
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use std::error::Error;
use std::path::Path;
use uuid::Uuid;

pub(super) async fn run_artifacts(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        Some("list") => list_artifacts(cfg, context).await,
        Some("get") => get_artifact(cfg, context).await,
        Some("content") => artifact_content(cfg, context).await,
        Some(other) => Err(format!("unknown artifacts subcommand: {other}").into()),
        None => Err("artifacts requires list|get|content".into()),
    }
}

async fn list_artifacts(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let request = ArtifactListRequest {
        source_id: flag_value(cfg, "--source-id").map(SourceId::new),
        job_id: flag_value(cfg, "--job-id")
            .map(|id| Uuid::parse_str(&id).map(JobId::new))
            .transpose()?,
        kind: flag_value(cfg, "--kind")
            .map(|kind| parse_wire_enum::<ArtifactKind>("artifact kind", &kind))
            .transpose()?,
        limit: parse_u32_flag(cfg, "--limit")?,
        cursor: flag_value(cfg, "--cursor"),
    };
    let page = axon_services::artifacts::list_artifacts(context, request)
        .await
        .map_err(api_error)?;
    print_value(page)
}

async fn get_artifact(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let artifact_id = ArtifactId::new(positional(cfg, 1, "artifact_id")?);
    let detail = axon_services::artifacts::get_artifact(context, artifact_id)
        .await
        .map_err(api_error)?;
    print_value(detail)
}

async fn artifact_content(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    if flag_value(cfg, "--range").is_some() {
        return Err("artifact byte ranges are not implemented by the local store".into());
    }
    let artifact_id = ArtifactId::new(positional(cfg, 1, "artifact_id")?);
    let content = axon_services::artifacts::artifact_content(context, artifact_id)
        .await
        .map_err(api_error)?;
    let bytes = tokio::fs::read(&content.path).await?;
    let output = flag_value(cfg, "--output").map(std::path::PathBuf::from);
    let download = cfg.positional.iter().any(|value| value == "--download");
    if let Some(path) = output
        .or_else(|| download.then(|| std::path::PathBuf::from(default_artifact_filename(&content))))
    {
        axon_core::artifacts::atomic_write_explicit(&path, &bytes)
            .await
            .map_err(|error| error.to_string())?;
        return print_value(serde_json::json!({
            "artifact_id": content.artifact_id,
            "content_type": content.content_type,
            "size_bytes": bytes.len(),
            "output": path,
        }));
    }
    if cfg.json_output {
        return print_value(serde_json::json!({
            "artifact_id": content.artifact_id,
            "content_type": content.content_type,
            "size_bytes": bytes.len(),
        }));
    }
    if content.content_type.starts_with("text/") || content.content_type.contains("json") {
        print!("{}", String::from_utf8(bytes)?);
        Ok(())
    } else {
        Err("binary artifact content requires --output or --download".into())
    }
}

pub(super) async fn run_uploads(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        Some("list") => list_uploads(cfg, context).await,
        Some("get") => get_upload(cfg, context).await,
        Some("create") => create_upload(cfg, context).await,
        Some("complete") => complete_upload(cfg, context).await,
        Some("abort") => abort_upload(cfg, context).await,
        Some(other) => Err(format!("unknown uploads subcommand: {other}").into()),
        None => Err("uploads requires list|get|create|complete|abort".into()),
    }
}

async fn list_uploads(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let page = axon_services::uploads::list_uploads(
        context,
        UploadListRequest {
            status: flag_value(cfg, "--status")
                .map(|status| parse_wire_enum::<UploadStatusKind>("upload status", &status))
                .transpose()?,
            limit: parse_u32_flag(cfg, "--limit")?,
            cursor: flag_value(cfg, "--cursor"),
        },
    )
    .await
    .map_err(api_error)?;
    print_value(page)
}

async fn get_upload(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let status = axon_services::uploads::get_upload(
        context,
        UploadId::new(positional(cfg, 1, "upload_id")?),
    )
    .await
    .map_err(api_error)?;
    print_value(status)
}

async fn create_upload(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let path = std::path::PathBuf::from(positional(cfg, 1, "path")?);
    let metadata = tokio::fs::metadata(&path).await?;
    if !metadata.is_file() {
        return Err("upload path must be a regular file".into());
    }
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("upload path has no valid filename")?
        .to_string();
    let purpose = parse_wire_enum::<UploadPurpose>(
        "upload purpose",
        flag_value(cfg, "--purpose")
            .as_deref()
            .unwrap_or("source_artifact"),
    )?;
    let created = axon_services::uploads::create_upload(
        context,
        UploadCreateRequest {
            filename,
            content_type: content_type_for(&path).to_string(),
            size_bytes: metadata.len(),
            purpose,
            sha256: None,
            source_hint: flag_value(cfg, "--source-hint"),
            source_id: None,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .map_err(api_error)?;
    let bytes = tokio::fs::read(&path).await?;
    let status = axon_services::uploads::put_upload_content(
        context,
        created.upload_id.clone(),
        bytes,
        Some(content_type_for(&path).to_string()),
        None,
    )
    .await
    .map_err(api_error)?;
    print_value(serde_json::json!({ "upload": created, "status": status }))
}

async fn complete_upload(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let mut source_options = MetadataMap::new();
    for option in flag_values(cfg, "--source-option") {
        let (key, value) = option
            .split_once('=')
            .ok_or("--source-option must use KEY=VALUE")?;
        if key.trim().is_empty() {
            return Err("--source-option key must not be empty".into());
        }
        let value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        source_options.insert(key.trim().to_string(), value);
    }
    let result = axon_services::uploads::complete_upload(
        context,
        UploadId::new(positional(cfg, 1, "upload_id")?),
        UploadCompleteRequest {
            sha256: flag_value(cfg, "--sha256"),
            source_options,
        },
    )
    .await
    .map_err(api_error)?;
    print_value(result)
}

async fn abort_upload(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let result = axon_services::uploads::abort_upload(
        context,
        UploadId::new(positional(cfg, 1, "upload_id")?),
        UploadAbortRequest {
            reason: flag_value(cfg, "--reason"),
        },
    )
    .await
    .map_err(api_error)?;
    print_value(result)
}

fn parse_wire_enum<T: serde::de::DeserializeOwned>(
    label: &str,
    value: &str,
) -> Result<T, Box<dyn Error>> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("invalid {label} `{value}`: {error}").into())
}

fn api_error(error: axon_api::source::ApiError) -> Box<dyn Error> {
    error.to_string().into()
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("json") => "application/json",
        Some("md" | "txt" | "log" | "csv") => "text/plain; charset=utf-8",
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn default_artifact_filename(content: &axon_services::artifacts::ArtifactContentFile) -> String {
    let extension = match content.content_type.as_str() {
        "application/json" => "json",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "application/pdf" => "pdf",
        value if value.starts_with("text/") => "txt",
        _ => "bin",
    };
    format!("{}.{}", content.artifact_id.0, extension)
}
