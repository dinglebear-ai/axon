use super::*;

#[tokio::test]
#[cfg(unix)]
async fn failed_latest_publication_preserves_previous_committed_view() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let latest = temp.path().join("latest");
    tokio::fs::create_dir_all(&source).await.unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(source.join("manifest.jsonl")).unwrap();
    tokio::fs::create_dir_all(&latest).await.unwrap();
    tokio::fs::write(latest.join("committed.txt"), "known-good")
        .await
        .unwrap();

    update_latest_reflink(&source, &latest)
        .await
        .expect_err("copying a directory as the manifest must fail");

    assert_eq!(
        tokio::fs::read_to_string(latest.join("committed.txt"))
            .await
            .unwrap(),
        "known-good"
    );
}

#[tokio::test]
async fn early_markdown_read_failure_removes_latest_staging_directory() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let latest = temp.path().join("latest");
    tokio::fs::create_dir_all(&source).await.unwrap();
    tokio::fs::write(source.join("markdown"), "not a directory")
        .await
        .unwrap();

    update_latest_reflink(&source, &latest)
        .await
        .expect_err("reading a regular file as the markdown directory must fail");

    let mut entries = tokio::fs::read_dir(temp.path()).await.unwrap();
    let mut leaked_staging = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".latest.staging-") {
            leaked_staging.push(name);
        }
    }
    assert!(
        leaked_staging.is_empty(),
        "failed publication leaked staging directories: {leaked_staging:?}"
    );
}
