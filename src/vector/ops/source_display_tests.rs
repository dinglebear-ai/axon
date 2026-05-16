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
