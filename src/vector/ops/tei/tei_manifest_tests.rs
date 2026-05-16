use super::read_manifest_url_map;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn read_manifest_url_map_maps_markdown_file_to_url() {
    let root = std::env::temp_dir().join(format!("axon-tei-manifest-test-{}", Uuid::new_v4()));
    let markdown_dir = root.join("markdown");
    fs::create_dir_all(&markdown_dir).expect("create markdown dir");

    let markdown_file = markdown_dir.join("001-example.md");
    fs::write(&markdown_file, "# test").expect("write markdown file");

    let manifest_path = root.join("manifest.jsonl");
    let line = serde_json::json!({
        "url": "https://example.com/docs",
        "file_path": markdown_file.to_string_lossy(),
        "markdown_chars": 42
    });
    fs::write(&manifest_path, format!("{line}\n")).expect("write manifest");

    let mapped = read_manifest_url_map(&markdown_dir);
    let key = fs::canonicalize(&markdown_file).unwrap_or_else(|_| PathBuf::from(&markdown_file));
    assert_eq!(
        mapped.get(&key).map(|(u, _)| u.as_str()),
        Some("https://example.com/docs")
    );

    let _ = fs::remove_dir_all(&root);
}
