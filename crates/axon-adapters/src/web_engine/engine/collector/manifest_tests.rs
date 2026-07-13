use super::super::page::PageOutcome;
use super::{previous_markdown_path, write_page_to_manifest};
use crate::web_engine::manifest::ManifestEntry;
use std::collections::HashMap;

fn entry(relative_path: &str) -> ManifestEntry {
    ManifestEntry {
        url: "https://example.com".to_string(),
        relative_path: relative_path.to_string(),
        markdown_chars: 10,
        content_hash: Some("hash".to_string()),
        changed: false,
        structured: None,
    }
}

#[test]
fn previous_markdown_path_resolves_manifest_markdown_under_archive_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown_dir = tmp.path().join("markdown");
    let path = previous_markdown_path(&markdown_dir, &entry("markdown/0001-example.md"));
    assert_eq!(path, Some(tmp.path().join("markdown.old/0001-example.md")));
}

#[test]
fn previous_markdown_path_rejects_empty_archive_relative_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown_dir = tmp.path().join("markdown");
    assert_eq!(
        previous_markdown_path(&markdown_dir, &entry("markdown")),
        None
    );
    assert_eq!(
        previous_markdown_path(&markdown_dir, &entry("markdown/")),
        None
    );
}

#[test]
fn previous_markdown_path_rejects_absolute_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown_dir = tmp.path().join("markdown");
    let absolute = tmp.path().join("custom.md");
    let path = previous_markdown_path(&markdown_dir, &entry(&absolute.to_string_lossy()));
    assert_eq!(path, None);
}

#[test]
fn previous_markdown_path_rejects_traversal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown_dir = tmp.path().join("markdown");
    let path = previous_markdown_path(&markdown_dir, &entry("markdown/../secret.md"));
    assert_eq!(path, None);
}

#[tokio::test]
async fn reused_page_copies_from_markdown_old_archive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown_dir = tmp.path().join("markdown");
    let archive_dir = tmp.path().join("markdown.old");
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&archive_dir).await.unwrap();
    tokio::fs::write(archive_dir.join("0001-example.md"), b"archived markdown")
        .await
        .unwrap();

    let url = "https://example.com";
    let previous = entry("markdown/0001-example.md");
    let mut previous_manifest = HashMap::new();
    previous_manifest.insert(url.to_string(), previous.clone());

    let output_manifest = tokio::fs::File::create(tmp.path().join("manifest.jsonl"))
        .await
        .unwrap();
    let mut manifest = tokio::io::BufWriter::new(output_manifest);
    let outcome = PageOutcome::Reused {
        filename: "0001-example.md".to_string(),
        trimmed: "fresh fallback markdown".to_string(),
        entry: previous,
    };

    let wrote = write_page_to_manifest(
        &mut manifest,
        &outcome,
        &markdown_dir,
        &previous_manifest,
        url,
    )
    .await
    .unwrap();

    assert!(wrote);
    let copied = tokio::fs::read_to_string(markdown_dir.join("0001-example.md"))
        .await
        .unwrap();
    assert_eq!(copied, "archived markdown");
}
