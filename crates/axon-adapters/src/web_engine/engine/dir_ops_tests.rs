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

#[tokio::test]
async fn sync_tree_reports_all_files_and_directories_in_one_batch() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("nested");
    tokio::fs::create_dir_all(&nested).await.unwrap();
    for index in 0..64 {
        tokio::fs::write(nested.join(format!("{index}.md")), "durable")
            .await
            .unwrap();
    }

    let stats = sync_tree(temp.path()).await.unwrap();

    assert_eq!(stats.files, 64);
    assert_eq!(stats.directories, 2);
}

#[tokio::test]
async fn cleanup_failure_after_exchange_keeps_the_new_committed_view() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let latest = temp.path().join("latest");
    tokio::fs::create_dir_all(source.join("markdown"))
        .await
        .unwrap();
    tokio::fs::write(source.join("markdown/new.md"), "new")
        .await
        .unwrap();
    tokio::fs::create_dir_all(&latest).await.unwrap();
    tokio::fs::write(latest.join("old.md"), "old")
        .await
        .unwrap();

    update_latest_reflink_with_failure(&source, &latest, Some(LatestFailurePoint::Cleanup))
        .await
        .expect("cleanup after the commit point is deferred, not publication failure");

    assert_eq!(
        tokio::fs::read_to_string(latest.join("markdown/new.md"))
            .await
            .unwrap(),
        "new"
    );
    assert!(!latest.join("old.md").exists());
    let mut entries = tokio::fs::read_dir(temp.path()).await.unwrap();
    let mut cleanup_debt = false;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        cleanup_debt |= entry
            .file_name()
            .to_string_lossy()
            .starts_with(".latest.staging-");
    }
    assert!(
        cleanup_debt,
        "replaced view remains identifiable for later cleanup"
    );
}

#[tokio::test]
async fn later_publication_retries_only_validated_latest_cleanup_debt() {
    let temp = tempfile::tempdir().unwrap();
    let first_source = temp.path().join("first-source");
    let second_source = temp.path().join("second-source");
    let latest = temp.path().join("latest");
    tokio::fs::create_dir_all(first_source.join("markdown"))
        .await
        .unwrap();
    tokio::fs::write(first_source.join("markdown/first.md"), "first")
        .await
        .unwrap();
    tokio::fs::create_dir_all(second_source.join("markdown"))
        .await
        .unwrap();
    tokio::fs::write(second_source.join("markdown/second.md"), "second")
        .await
        .unwrap();
    tokio::fs::create_dir_all(&latest).await.unwrap();
    tokio::fs::write(latest.join("old.md"), "old")
        .await
        .unwrap();

    update_latest_reflink_with_failure(&first_source, &latest, Some(LatestFailurePoint::Cleanup))
        .await
        .expect("the first publication commits despite injected cleanup failure");

    let unrelated = temp
        .path()
        .join(".latest.staging-00000000-0000-4000-8000-000000000000");
    tokio::fs::create_dir_all(&unrelated).await.unwrap();
    tokio::fs::write(unrelated.join("keep"), "unrelated")
        .await
        .unwrap();

    update_latest_reflink(&second_source, &latest)
        .await
        .expect("a later publication retries validated cleanup debt");

    assert_eq!(
        tokio::fs::read_to_string(latest.join("markdown/second.md"))
            .await
            .unwrap(),
        "second"
    );
    assert!(
        unrelated.join("keep").exists(),
        "unmarked paths are preserved"
    );

    let mut entries = tokio::fs::read_dir(temp.path()).await.unwrap();
    let mut marked_debt = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".latest.staging-")
            && entry.path().join(LATEST_CLEANUP_DEBT_MARKER).exists()
        {
            marked_debt.push(name);
        }
    }
    assert!(
        marked_debt.is_empty(),
        "later publication left validated cleanup debt: {marked_debt:?}"
    );
}

#[tokio::test]
async fn initial_parent_sync_failure_does_not_publish_latest_view() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let latest = temp.path().join("latest");
    tokio::fs::create_dir_all(source.join("markdown"))
        .await
        .unwrap();
    tokio::fs::write(source.join("markdown/new.md"), "new")
        .await
        .unwrap();

    update_latest_reflink_with_failure(&source, &latest, Some(LatestFailurePoint::ParentSync))
        .await
        .expect_err("a failed initial publication sync must be reported");

    assert!(
        !latest.exists(),
        "an initial view whose parent entry was not synced must be rolled back"
    );
}
