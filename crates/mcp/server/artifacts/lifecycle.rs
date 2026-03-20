use super::super::common::{internal_error, invalid_params};
use super::path::ensure_artifact_root;
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
    modified_secs_ago: u64,
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
            files.push(ArtifactFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                relative_path,
                path,
                bytes: meta.len(),
                modified_secs_ago,
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
            serde_json::json!({
                "name": file.name,
                "relative_path": file.relative_path,
                "bytes": file.bytes,
                "modified_secs_ago": file.modified_secs_ago,
                "path": file.path,
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
    let mut candidates: Vec<serde_json::Value> = Vec::new();
    for file in all_files {
        let age_secs = file.modified_secs_ago;
        if age_secs >= cutoff_secs {
            let age_hours = (age_secs as f64 / 3600.0 * 10.0).round() / 10.0;
            candidates.push(serde_json::json!({
                "name": file.name,
                "relative_path": file.relative_path,
                "path": file.path,
                "age_hours": age_hours,
                "bytes": file.bytes,
            }));
        }
    }
    let would_free: u64 = candidates.iter().filter_map(|c| c["bytes"].as_u64()).sum();
    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "max_age_hours": max_age_hours,
            "scanned": scanned,
            "would_delete": candidates.len(),
            "would_free_bytes": would_free,
            "files": candidates,
        }));
    }
    let mut deleted = 0u64;
    let mut freed = 0u64;
    let mut errors: Vec<serde_json::Value> = Vec::new();
    for candidate in &candidates {
        let path_str = candidate["path"].as_str().unwrap_or("");
        match tokio::fs::remove_file(path_str).await {
            Ok(_) => {
                deleted += 1;
                freed += candidate["bytes"].as_u64().unwrap_or(0);
            }
            Err(e) => errors.push(serde_json::json!({ "path": path_str, "error": e.to_string() })),
        }
    }
    Ok(serde_json::json!({
        "dry_run": false,
        "max_age_hours": max_age_hours,
        "scanned": scanned,
        "deleted": deleted,
        "freed_bytes": freed,
        "errors": errors,
        "files": candidates,
    }))
}

/// Maximum file size (in bytes) that `search_artifact_files` will read.
/// Files larger than this are skipped to avoid memory pressure.
const MAX_SEARCH_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

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
                        "file": relative_path,
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
