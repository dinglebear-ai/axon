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
mod tests {
    use super::display_source;
    use std::fs;

    #[test]
    fn display_source_keeps_http_urls() {
        let url = "https://example.com/docs";
        assert_eq!(display_source(url), url);
    }

    #[test]
    fn display_source_resolves_relative_path_via_manifest() {
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let markdown_dir = tmp.path().join("markdown");
        fs::create_dir_all(&markdown_dir).expect("create markdown dir");
        let md_file = markdown_dir.join("0001-example-com-docs.md");
        fs::write(&md_file, "# docs").expect("write md file");
        let manifest = tmp.path().join("manifest.jsonl");
        fs::write(
            &manifest,
            format!(
                "{}\n",
                serde_json::json!({
                    "url": "https://example.com/docs",
                    "relative_path": "markdown/0001-example-com-docs.md",
                    "markdown_chars": 6
                })
            ),
        )
        .expect("write manifest");

        let result = display_source(&md_file.to_string_lossy());
        assert_eq!(result, "https://example.com/docs");
    }

    // File is not inside a `markdown/` subdirectory, so find_manifest_for_markdown returns None.
    // The tempdir also has no .git ancestor, so infer_repo_label returns None.
    // display_source must fall back to returning the raw path string unchanged.
    #[test]
    fn display_source_falls_back_to_raw_path_when_no_manifest_and_no_git() {
        let raw = "some_doc.md";
        assert_eq!(display_source(raw), raw);
    }

    // Manifest uses "file_path" (absolute path) instead of "relative_path".
    // display_source must still resolve it to the URL in the manifest entry.
    #[test]
    fn display_source_with_absolute_path_format_in_manifest() {
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let markdown_dir = tmp.path().join("markdown");
        fs::create_dir_all(&markdown_dir).expect("create markdown dir");
        let md_file = markdown_dir.join("0002-absolute-example.md");
        fs::write(&md_file, "# absolute").expect("write md file");

        // Resolve to canonical path so the manifest entry matches what normalize_path returns.
        let canonical = fs::canonicalize(&md_file).expect("canonicalize");
        let manifest = tmp.path().join("manifest.jsonl");
        fs::write(
            &manifest,
            format!(
                "{}\n",
                serde_json::json!({
                    "url": "https://example.com/absolute",
                    "file_path": canonical.to_string_lossy(),
                    "markdown_chars": 8
                })
            ),
        )
        .expect("write manifest");

        let result = display_source(&md_file.to_string_lossy());
        assert_eq!(result, "https://example.com/absolute");
    }

    // A manifest entry that matches the file path but has an empty "url" field must be skipped.
    // The second entry (with a valid URL) must be returned.
    #[test]
    fn display_source_skips_manifest_entry_with_empty_url() {
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let markdown_dir = tmp.path().join("markdown");
        fs::create_dir_all(&markdown_dir).expect("create markdown dir");
        let md_file = markdown_dir.join("0003-skip-empty-url.md");
        fs::write(&md_file, "# skip").expect("write md file");

        // First entry: correct relative_path but url is "".
        // Second entry: same relative_path with a valid url — must be returned.
        let rel = "markdown/0003-skip-empty-url.md";
        let manifest = tmp.path().join("manifest.jsonl");
        let line1 = serde_json::json!({
            "url": "",
            "relative_path": rel
        });
        let line2 = serde_json::json!({
            "url": "https://example.com/real",
            "relative_path": rel
        });
        fs::write(&manifest, format!("{line1}\n{line2}\n")).expect("write manifest");

        let result = display_source(&md_file.to_string_lossy());
        assert_eq!(result, "https://example.com/real");
    }

    // Any string starting with "http://" (non-TLS) is returned as-is without any filesystem lookup.
    #[test]
    fn display_source_http_url_not_looked_up_in_manifest() {
        let url = "http://internal.example.com/api/docs";
        assert_eq!(display_source(url), url);
    }
}
