use super::{
    ArtifactHandle, ArtifactKind, atomic_write_explicit, atomic_write_under,
    write_configured_output, write_managed_output,
};
use uuid::Uuid;

#[test]
fn artifact_handle_rejects_parent_dir_relative_path() {
    let result = ArtifactHandle::new(
        ArtifactKind::Markdown,
        "../secret.md",
        Some("https://example.com".to_string()),
        None,
        "abc123".to_string(),
        12,
        Some(1),
        None,
    );

    assert!(result.is_err());
}

#[test]
fn artifact_id_is_stable_for_kind_path_and_hash() {
    let one = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        Some("/home/axon/.axon/output/domains/example.com/job-1/manifest.jsonl".to_string()),
    )
    .expect("artifact handle");
    let two = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        None,
    )
    .expect("artifact handle");

    assert_eq!(one.artifact_id, two.artifact_id);
    assert_eq!(one.kind, ArtifactKind::CrawlManifest);
}

#[tokio::test]
async fn atomic_write_under_rejects_parent_traversal() {
    let temp = tempfile::TempDir::new().expect("tempdir");

    let err = atomic_write_under(temp.path(), "../escape.txt", b"secret")
        .await
        .expect_err("parent traversal must be rejected");

    assert!(err.to_string().contains("unsafe artifact relative_path"));
    assert!(!temp.path().join("../escape.txt").exists());
}

#[tokio::test]
async fn atomic_write_under_cleans_temp_file_on_create_failure() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let nested = temp.path().join("nested");
    std::fs::create_dir_all(&nested).expect("nested dir");
    std::fs::create_dir(nested.join("output.md")).expect("rename blocker");

    let err = atomic_write_under(temp.path(), "nested/output.md", b"content")
        .await
        .expect_err("rename into existing directory must fail");

    assert!(err.to_string().contains("rename temp file"));
    let temp_files = std::fs::read_dir(&nested)
        .expect("read nested dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        .count();
    assert_eq!(temp_files, 0, "failed writes must not leave temp artifacts");
}

#[tokio::test]
async fn atomic_write_under_writes_relative_path_inside_root() {
    let temp = tempfile::TempDir::new().expect("tempdir");

    let path = atomic_write_under(temp.path(), "screenshots/shot.png", b"png")
        .await
        .expect("write artifact");

    assert_eq!(path, temp.path().join("screenshots/shot.png"));
    assert_eq!(std::fs::read(path).expect("read artifact"), b"png");
}

#[tokio::test]
async fn atomic_write_explicit_replaces_an_existing_file_without_temp_leaks() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("state.json");
    tokio::fs::write(&path, b"old").await.expect("seed file");

    atomic_write_explicit(&path, b"new")
        .await
        .expect("replace existing file");

    assert_eq!(tokio::fs::read(&path).await.expect("read file"), b"new");
    let entries = std::fs::read_dir(temp.path())
        .expect("read tempdir")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, [std::ffi::OsString::from("state.json")]);
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_write_under_rejects_symlinked_parent_escape() {
    use std::os::unix::fs::symlink;

    let root = tempfile::TempDir::new().expect("root");
    let outside = tempfile::TempDir::new().expect("outside");
    symlink(outside.path(), root.path().join("redirect")).expect("symlink");

    let err = atomic_write_under(root.path(), "redirect/escape.txt", b"secret")
        .await
        .expect_err("symlinked parent must not escape root");

    assert!(err.to_string().contains("symlink"));
    assert!(!outside.path().join("escape.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_write_under_rejects_symlinked_parent_before_creating_missing_children() {
    use std::os::unix::fs::symlink;

    let root = tempfile::TempDir::new().expect("root");
    let outside = tempfile::TempDir::new().expect("outside");
    symlink(outside.path(), root.path().join("redirect")).expect("symlink");

    let err = atomic_write_under(root.path(), "redirect/newdir/escape.txt", b"secret")
        .await
        .expect_err("symlinked parent must be rejected before create_dir");

    assert!(err.to_string().contains("symlink"));
    assert!(
        !outside.path().join("newdir").exists(),
        "writer must not create directories through a symlink before rejection"
    );
}

#[tokio::test]
async fn write_configured_output_preserves_explicit_path_outside_output_dir() {
    let output_root = tempfile::TempDir::new().expect("output root");
    let explicit_root = tempfile::TempDir::new().expect("explicit root");
    let explicit = explicit_root.path().join("shot.png");

    let path = write_configured_output(
        output_root.path(),
        Some(&explicit),
        "screenshots/shot.png",
        b"png",
    )
    .await
    .expect("explicit write");

    assert_eq!(path, explicit);
    assert_eq!(std::fs::read(explicit).expect("read explicit"), b"png");
    assert!(!output_root.path().join("screenshots/shot.png").exists());
}

#[tokio::test]
async fn write_managed_output_keeps_path_under_output_root() {
    let output_root = tempfile::TempDir::new().expect("output root");
    let managed = output_root.path().join("screenshots/shot.png");

    let path = write_managed_output(output_root.path(), &managed, b"png")
        .await
        .expect("managed write");

    assert_eq!(path, managed);
    assert_eq!(std::fs::read(managed).expect("read managed"), b"png");
}

#[tokio::test]
async fn write_managed_output_rejects_path_outside_output_root() {
    let output_root = tempfile::TempDir::new().expect("output root");
    let outside = tempfile::TempDir::new().expect("outside");

    let err = write_managed_output(output_root.path(), outside.path().join("shot.png"), b"png")
        .await
        .expect_err("outside managed path must be rejected");

    assert!(err.to_string().contains("escaped output root"));
}

#[tokio::test]
async fn atomic_write_explicit_writes_outside_managed_root() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let path = temp.path().join("explicit.txt");

    let written = atomic_write_explicit(&path, b"content")
        .await
        .expect("explicit write");

    assert_eq!(written, path);
    assert_eq!(std::fs::read(path).expect("read explicit"), b"content");
}
