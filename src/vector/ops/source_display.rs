use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn find_manifest_for_markdown(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    if parent.file_name().and_then(|s| s.to_str()) != Some("markdown") {
        return None;
    }
    let manifest = parent.parent()?.join("manifest.jsonl");
    manifest.exists().then_some(manifest)
}

fn manifest_url_for_file(path: &Path, manifest: &Path) -> Option<String> {
    let file = File::open(manifest).ok()?;
    let manifest_dir = manifest.parent();
    let target = normalize_path(path);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Handle both "relative_path" (modern crawl format) and "file_path" (absolute path format).
        let manifest_path = if let Some(rel) = parsed
            .get("relative_path")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            let base = manifest_dir.unwrap_or(Path::new("."));
            normalize_path(&base.join(rel))
        } else if let Some(abs) = parsed
            .get("file_path")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            normalize_path(Path::new(abs))
        } else {
            continue;
        };
        if manifest_path == target
            && let Some(url) = parsed.get("url").and_then(Value::as_str)
            && !url.is_empty()
        {
            return Some(url.to_string());
        }
    }
    None
}

fn lookup_manifest_url(path: &Path) -> Option<String> {
    let manifest = find_manifest_for_markdown(path)?;
    manifest_url_for_file(path, &manifest)
}

fn infer_repo_label(path: &Path) -> Option<String> {
    let normalized = normalize_path(path);
    let mut cursor = normalized.parent()?;
    loop {
        let git_marker = cursor.join(".git");
        if git_marker.exists() {
            let repo = cursor.file_name()?.to_string_lossy().to_string();
            let rel = normalized
                .strip_prefix(cursor)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized.to_string_lossy().to_string());
            return Some(format!("{repo}/{rel}"));
        }
        cursor = cursor.parent()?;
    }
}

pub fn display_source(value: &str) -> String {
    if is_http_url(value) {
        return value.to_string();
    }
    let path = Path::new(value);
    if let Some(mapped_url) = lookup_manifest_url(path) {
        return mapped_url;
    }
    if let Some(repo_label) = infer_repo_label(path) {
        return format!("{value} (repo:{repo_label})");
    }
    value.to_string()
}

#[cfg(test)]
#[path = "source_display_tests.rs"]
mod tests;
