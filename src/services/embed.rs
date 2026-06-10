//! Service-layer wrappers for embed job lifecycle operations and synchronous embedding entry points.

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::config_snapshot::config_snapshot_json;
use crate::services::context::ServiceContext;
use crate::services::events::{ServiceEvent, is_secret_like};
use crate::services::jobs as job_service;
use crate::services::runtime::ServiceJobRuntime;
use crate::services::runtime::WorkerMode;
use crate::services::types::{
    EmbedJobResult, EmbedStartResult, ExecutionMode, JobStartOutcome, StartDisposition,
};
use crate::vector::ops::input::classify::path_extension;
use crate::vector::ops::input::select;
use crate::vector::ops::{embed_path_native, embed_path_native_with_progress};
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

const EMBED_ALLOWED_ROOTS_ENV: &str = "AXON_MCP_EMBED_ALLOWED_ROOTS";
type EmbedValidationError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct EmbedValidationLimits {
    max_file_bytes: u64,
    max_depth: usize,
    max_entries: usize,
}

#[derive(Debug, Default)]
struct EmbedDirectoryScan {
    entries_seen: usize,
}

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_embed_start_result(job_id: String) -> EmbedStartResult {
    EmbedStartResult { job_id }
}

pub fn map_embed_job_result(payload: serde_json::Value) -> EmbedJobResult {
    EmbedJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn embed_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<EmbedJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    Ok(job.map(|value| {
        map_embed_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn embed_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Embed, limit, offset).await?;
    Ok(map_embed_job_result(serde_json::to_value(jobs)?))
}

pub async fn embed_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Embed, id).await
}

pub async fn embed_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Embed).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

pub async fn embed_start_with_context(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    _source_type: Option<&str>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    // tx is accepted for API compatibility
    let _ = tx;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Embed {
            input: input.to_string(),
            config_json: config_snapshot_json(cfg)?,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;

    if !cfg.wait {
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::InProcess,
            result: map_embed_start_result(job_id.to_string()),
        });
    }

    wait_for_embed_completion(service_context.jobs.as_ref(), job_id).await?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Completed,
        execution_mode: ExecutionMode::InProcess,
        result: map_embed_start_result(job_id.to_string()),
    })
}

/// Enqueue an embed job for the input specified in cfg and return its job ID
/// immediately. The embed input is resolved from cfg.positional or cfg.output_dir
/// following the same logic as the CLI embed command.
pub async fn embed_start(
    cfg: &Config,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    let input = cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    });
    embed_start_with_context(cfg, &input, service_context, tx, None).await
}

pub async fn embed_now(cfg: &Config, input: &str) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native(cfg, input).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

pub async fn embed_now_with_source(
    cfg: &Config,
    input: &str,
    source_type: Option<&str>,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None, source_type).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

/// Validate embed input shared by REST and MCP-like server surfaces.
///
/// URL and free-text inputs are allowed. Existing local files/directories must
/// live under `AXON_MCP_EMBED_ALLOWED_ROOTS`; paths with dotfiles, secret-like
/// names, oversized files, or symlinks below a submitted directory are rejected
/// before an embed job can enqueue. Pruned directories (`select::is_pruned_dir`
/// — node_modules, target, …) are exempt from those checks: the reader never
/// visits them, and they routinely contain symlinks by design.
pub fn validate_server_embed_input_with_config(
    cfg: &Config,
    input: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    validate_server_embed_input_with_roots(
        input,
        &cfg.mcp_embed_allowed_roots,
        EmbedValidationLimits {
            max_file_bytes: cfg.mcp_embed_max_local_bytes,
            max_depth: cfg.mcp_embed_max_local_depth,
            max_entries: cfg.mcp_embed_max_local_entries,
        },
    )
}

fn validate_server_embed_input_with_roots(
    input: &str,
    allowed_roots: &[PathBuf],
    limits: EmbedValidationLimits,
) -> Result<String, EmbedValidationError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(embed_validation_error("input is required for embed"));
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        crate::core::http::validate_url(input).map_err(|err| err.to_string())?;
        return Ok(input.to_string());
    }
    let path = Path::new(input);
    if !path.exists() {
        if looks_path_like_input(input) {
            return Err(embed_validation_error(format!(
                "local embed path does not exist: {input}"
            )));
        }
        return Ok(input.to_string());
    }
    if allowed_roots.is_empty() {
        return Err(embed_validation_error(format!(
            "local file embedding is disabled; set {EMBED_ALLOWED_ROOTS_ENV} to allow specific roots"
        )));
    }
    let canonical =
        std::fs::canonicalize(path).map_err(|err| format!("invalid embed path: {err}"))?;
    let root = allowed_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| canonical.starts_with(root))
        .ok_or_else(|| {
            format!("local embed path must be under one of {EMBED_ALLOWED_ROOTS_ENV}")
        })?;
    validate_local_embed_entry(path, &canonical, &root, limits)?;
    Ok(canonical.to_string_lossy().to_string())
}

fn embed_validation_error(message: impl Into<String>) -> EmbedValidationError {
    Box::<dyn Error + Send + Sync>::from(message.into())
}

fn looks_path_like_input(input: &str) -> bool {
    select::looks_path_like(input)
}

fn validate_local_embed_entry(
    original: &Path,
    canonical: &Path,
    allowed_root: &Path,
    limits: EmbedValidationLimits,
) -> Result<(), EmbedValidationError> {
    let link_meta = std::fs::symlink_metadata(original)
        .map_err(|err| format!("invalid embed path metadata: {err}"))?;
    if link_meta.file_type().is_symlink() {
        return Err(embed_validation_error(
            "local embed path must not be a symlink",
        ));
    }
    validate_local_embed_relative_path(canonical, allowed_root)?;
    let meta = std::fs::metadata(canonical)
        .map_err(|err| format!("invalid embed path metadata: {err}"))?;
    if meta.is_file() {
        return validate_local_embed_file(canonical, allowed_root, meta.len(), limits);
    }
    if meta.is_dir() {
        let mut scan = EmbedDirectoryScan::default();
        validate_local_embed_directory(canonical, allowed_root, limits, &mut scan, 0)?;
        return Ok(());
    }
    Err(embed_validation_error(
        "local embed path must be a regular file or directory",
    ))
}

fn validate_local_embed_directory(
    directory: &Path,
    allowed_root: &Path,
    limits: EmbedValidationLimits,
    scan: &mut EmbedDirectoryScan,
    depth: usize,
) -> Result<(), EmbedValidationError> {
    if depth > limits.max_depth {
        return Err(embed_validation_error(format!(
            "local embed directory validation exceeded max depth {}",
            limits.max_depth
        )));
    }
    for entry in
        std::fs::read_dir(directory).map_err(|err| format!("invalid embed directory: {err}"))?
    {
        scan.entries_seen += 1;
        if scan.entries_seen > limits.max_entries {
            return Err(embed_validation_error(format!(
                "local embed directory validation exceeded max entries {}",
                limits.max_entries
            )));
        }
        let entry = entry.map_err(|err| format!("invalid embed entry: {err}"))?;
        let child = entry.path();
        let child_meta = std::fs::symlink_metadata(&child)
            .map_err(|err| format!("invalid embed entry metadata: {err}"))?;
        let name = child.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Enumerate exactly the set the reader (collect_embed_files in
        // tei/prepare.rs) will read: prune the same VCS/build directories and
        // skip the same binary extensions BEFORE applying the server-only
        // security checks (symlink/dotfile/secret/size). Pruned subtrees contain
        // symlinks by design (node_modules/.bin/*), and the reader never visits
        // them — checking symlinks first failed server-side embeds of any JS
        // project over files that are never read. A symlink occupying a pruned
        // name is skipped too (the reader skips all symlinks).
        if select::is_pruned_dir(name) && !child_meta.is_file() {
            continue;
        }
        if child_meta.file_type().is_symlink() {
            return Err(embed_validation_error(
                "local embed directory must not contain symlinks",
            ));
        }
        let child_canonical =
            std::fs::canonicalize(&child).map_err(|err| format!("invalid embed path: {err}"))?;
        if child_meta.is_dir() {
            validate_local_embed_relative_path(&child_canonical, allowed_root)?;
            validate_local_embed_directory(
                &child_canonical,
                allowed_root,
                limits,
                scan,
                depth + 1,
            )?;
        } else if child_meta.is_file() {
            if select::is_binary_ext(path_extension(name)) {
                continue;
            }
            validate_local_embed_relative_path(&child_canonical, allowed_root)?;
            validate_local_embed_file(&child_canonical, allowed_root, child_meta.len(), limits)?;
        } else {
            return Err(embed_validation_error(
                "local embed directory must contain only files and directories",
            ));
        }
    }
    Ok(())
}

fn validate_local_embed_file(
    canonical: &Path,
    allowed_root: &Path,
    size: u64,
    limits: EmbedValidationLimits,
) -> Result<(), EmbedValidationError> {
    validate_local_embed_relative_path(canonical, allowed_root)?;
    if size > limits.max_file_bytes {
        return Err(embed_validation_error(format!(
            "local embed file exceeds {} byte limit",
            limits.max_file_bytes
        )));
    }
    Ok(())
}

fn validate_local_embed_relative_path(
    canonical: &Path,
    allowed_root: &Path,
) -> Result<(), EmbedValidationError> {
    let relative = canonical
        .strip_prefix(allowed_root)
        .map_err(|_| "local embed path is outside the allowed root".to_string())?;
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        let lower = name.to_ascii_lowercase();
        if name.starts_with('.') {
            return Err(embed_validation_error(
                "local embed path must not include dotfiles",
            ));
        }
        if is_secret_like(&lower) {
            return Err(embed_validation_error(
                "local embed path appears to contain secret material",
            ));
        }
    }
    Ok(())
}

async fn wait_for_embed_completion(
    runtime: &dyn ServiceJobRuntime,
    job_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let final_status = runtime
        .wait_for_job(job_id, JobKind::Embed)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    if final_status == "failed" {
        if let Ok(Some(err)) = runtime.job_errors(job_id, JobKind::Embed).await {
            return Err(format!("embed job {job_id} failed: {err}").into());
        }
        return Err(format!("embed job {job_id} failed").into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "embed_tests.rs"]
mod tests;
