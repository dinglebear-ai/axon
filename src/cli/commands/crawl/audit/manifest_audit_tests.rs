use super::{fnv1a64_hex, read_manifest_entries};

#[tokio::test]
async fn read_manifest_entries_rejects_out_of_bounds_file_paths() {
    let output_dir = tempfile::TempDir::new().expect("output tempdir");
    let outside_dir = tempfile::TempDir::new().expect("outside tempdir");
    let markdown_dir = output_dir.path().join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("create markdown dir");

    let in_bounds = markdown_dir.join("page.md");
    let outside = outside_dir.path().join("secret.md");
    tokio::fs::write(&in_bounds, "inside")
        .await
        .expect("write in-bounds file");
    tokio::fs::write(&outside, "outside")
        .await
        .expect("write out-of-bounds file");

    let manifest = format!(
        "{}\n{}\n",
        serde_json::json!({
            "url": "https://example.com/in",
            "file_path": in_bounds.to_string_lossy(),
            "markdown_chars": 6
        }),
        serde_json::json!({
            "url": "https://example.com/out",
            "file_path": outside.to_string_lossy(),
            "markdown_chars": 7
        })
    );
    tokio::fs::write(output_dir.path().join("manifest.jsonl"), manifest)
        .await
        .expect("write manifest");

    let entries = read_manifest_entries(output_dir.path())
        .await
        .expect("read manifest entries");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].fingerprint, fnv1a64_hex(b"inside"));
    assert_eq!(entries[1].fingerprint, "path-outside-output-dir");
}
