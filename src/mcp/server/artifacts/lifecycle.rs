use super::super::common::{internal_error, invalid_params};
use super::path::ensure_artifact_root;
use crate::services::types::ArtifactHandle;
use regex::Regex;
use rmcp::ErrorData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
struct ArtifactFile {
    name: String,
    relative_path: String,
    path: PathBuf,
    bytes: u64,
    line_count: Option<u64>,
    modified_secs_ago: u64,
    artifact_handle: ArtifactHandle,
}

/// Maximum file size (in bytes) that `search_artifact_files` will read.
/// Files larger than this are skipped to avoid memory pressure.
const MAX_SEARCH_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

fn artifact_kind(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => "json",
        Some("png") => "screenshot",
        Some("md") | Some("markdown") => "markdown",
        Some("txt") => "text",
        Some("jsonl") => "jsonl",
        _ => "artifact",
    }
}

async fn count_text_lines(path: &Path, bytes: u64) -> Option<u64> {
    if bytes > MAX_SEARCH_FILE_BYTES {
        return None;
    }
    let text = tokio::fs::read_to_string(path).await.ok()?;
    Some(text.lines().count() as u64)
}

async fn collect_artifact_files(root: &Path) -> Result<Vec<ArtifactFile>, ErrorData> {
    let now = SystemTime::now();
    let mut files: Vec<ArtifactFile> = Vec::new();
    let mut dirs = vec![root.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| internal_error(format!("failed to read artifact dir: {e}")))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| internal_error(e.to_string()))?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|e| internal_error(e.to_string()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let meta = entry
                .metadata()
                .await
                .map_err(|e| internal_error(e.to_string()))?;
            let modified_secs_ago = meta
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let relative_path = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            let bytes = meta.len();
            let line_count = count_text_lines(&path, bytes).await;
            let artifact_handle = ArtifactHandle::try_from_path(
                artifact_kind(&path),
                root,
                &path,
                bytes,
                line_count,
                None,
                None,
            )
            .unwrap_or_else(|| {
                ArtifactHandle::new(
                    artifact_kind(&path),
                    relative_path.clone(),
                    path.to_string_lossy().into_owned(),
                    bytes,
                    line_count,
                    None,
                    None,
                )
            });
            files.push(ArtifactFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                relative_path,
                path,
                bytes,
                line_count,
                modified_secs_ago,
                artifact_handle,
            });
        }
    }

    Ok(files)
}

pub async fn list_artifact_files(
    limit: usize,
    offset: usize,
) -> Result<serde_json::Value, ErrorData> {
    let root = ensure_artifact_root().await?;
    let mut files = collect_artifact_files(&root).await?;
    files.sort_by_key(|f| f.modified_secs_ago);
    let total_bytes: u64 = files.iter().map(|f| f.bytes).sum();
    let total_count = files.len();
    let page: Vec<_> = files
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|file| {
            let relative_path = file.relative_path.clone();
            serde_json::json!({
                "name": file.name,
                "artifact_handle": file.artifact_handle,
                "relative_path": relative_path,
                "path": relative_path,
                "display_path": file.path,
                "bytes": file.bytes,
                "line_count": file.line_count,
                "modified_secs_ago": file.modified_secs_ago,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "artifact_dir": root,
        "total_count": total_count,
        "count": page.len(),
        "offset": offset,
        "limit": limit,
        "total_bytes": total_bytes,
        "files": page,
    }))
}

pub async fn delete_artifact_file(path: &Path) -> Result<u64, ErrorData> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| internal_error(format!("failed to stat artifact: {e}")))?;
    let bytes = meta.len();
    tokio::fs::remove_file(path)
        .await
        .map_err(|e| internal_error(format!("failed to delete artifact: {e}")))?;
    Ok(bytes)
}

pub async fn clean_artifact_files(
    max_age_hours: u64,
    dry_run: bool,
) -> Result<serde_json::Value, ErrorData> {
    let root = ensure_artifact_root().await?;
    let cutoff_secs = max_age_hours * 3600;
    let all_files = collect_artifact_files(&root).await?;
    let scanned = all_files.len();
    let mut candidates: Vec<(PathBuf, serde_json::Value)> = Vec::new();
    for file in all_files {
        let age_secs = file.modified_secs_ago;
        if age_secs >= cutoff_secs {
            let age_hours = (age_secs as f64 / 3600.0 * 10.0).round() / 10.0;
            let relative_path = file.relative_path.clone();
            candidates.push((
                file.path.clone(),
                serde_json::json!({
                    "name": file.name,
                    "artifact_handle": file.artifact_handle,
                    "relative_path": relative_path,
                    "path": relative_path,
                    "display_path": file.path,
                    "age_hours": age_hours,
                    "bytes": file.bytes,
                    "line_count": file.line_count,
                }),
            ));
        }
    }
    let would_free: u64 = candidates
        .iter()
        .filter_map(|(_, c)| c["bytes"].as_u64())
        .sum();
    let candidate_values: Vec<serde_json::Value> =
        candidates.iter().map(|(_, value)| value.clone()).collect();
    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "max_age_hours": max_age_hours,
            "scanned": scanned,
            "would_delete": candidate_values.len(),
            "would_free_bytes": would_free,
            "files": candidate_values,
        }));
    }
    let mut deleted = 0u64;
    let mut freed = 0u64;
    let mut errors: Vec<serde_json::Value> = Vec::new();
    for (delete_path, candidate) in &candidates {
        match tokio::fs::remove_file(delete_path).await {
            Ok(_) => {
                deleted += 1;
                freed += candidate["bytes"].as_u64().unwrap_or(0);
            }
            Err(e) => errors.push(serde_json::json!({
                "path": candidate["path"].clone(),
                "display_path": delete_path,
                "error": e.to_string(),
            })),
        }
    }
    let candidate_values: Vec<serde_json::Value> =
        candidates.iter().map(|(_, value)| value.clone()).collect();
    Ok(serde_json::json!({
        "dry_run": false,
        "max_age_hours": max_age_hours,
        "scanned": scanned,
        "deleted": deleted,
        "freed_bytes": freed,
        "errors": errors,
        "files": candidate_values,
    }))
}

pub async fn search_artifact_files(
    pattern: &str,
    limit: usize,
) -> Result<serde_json::Value, ErrorData> {
    let re = Arc::new(
        Regex::new(pattern).map_err(|e| invalid_params(format!("invalid regex pattern: {e}")))?,
    );
    let root = ensure_artifact_root().await?;
    let files = collect_artifact_files(&root).await?;
    let files_scanned = files
        .iter()
        .filter(|f| f.bytes <= MAX_SEARCH_FILE_BYTES)
        .count();

    let sem = Arc::new(tokio::sync::Semaphore::new(8));
    let mut set: JoinSet<Vec<serde_json::Value>> = JoinSet::new();
    for file in files {
        // Skip files larger than 10 MB to avoid memory pressure from read_to_string.
        if file.bytes > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let re = Arc::clone(&re);
        let sem = Arc::clone(&sem);
        let relative_path = file.relative_path;
        let artifact_handle = file.artifact_handle;
        let path = file.path;
        set.spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return Vec::new(),
            };
            let text = match tokio::fs::read_to_string(&path).await {
                Ok(t) => t,
                Err(_) => return Vec::new(),
            };
            text.lines()
                .enumerate()
                .filter(|(_, line)| re.is_match(line))
                .map(|(idx, line)| {
                    serde_json::json!({
                        "artifact_handle": artifact_handle.clone(),
                        "file": relative_path.clone(),
                        "line": idx + 1,
                        "text": line,
                    })
                })
                .collect()
        });
    }

    let mut matches: Vec<serde_json::Value> = Vec::new();
    while let Some(result) = set.join_next().await {
        if let Ok(file_matches) = result {
            for m in file_matches {
                matches.push(m);
                if matches.len() >= limit {
                    set.abort_all();
                    break;
                }
            }
        }
    }

    Ok(serde_json::json!({
        "pattern": pattern,
        "files_scanned": files_scanned,
        "matches": matches,
        "limit": limit,
    }))
}
