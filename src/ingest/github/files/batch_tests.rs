use super::*;

#[test]
fn failed_batch_accounting_counts_batches_docs_and_chunks() {
    let mut stats = GitHubFileEmbedStats::default();

    stats.record_failed_batch(2, 3, 7);
    stats.record_failed_batch(1, 2, 5);

    assert_eq!(stats.failed_batches, 2);
    assert_eq!(stats.failed_files, 3);
    assert_eq!(stats.failed_docs, 5);
    assert_eq!(stats.failed_chunks, 12);
    assert!(stats.has_failures());
}

#[tokio::test]
async fn stale_cleanup_skips_partial_and_failed_runs() {
    let mut stats = GitHubFileEmbedStats::default();
    let ctx = FileEmbedCtx {
        cfg: Config::test_default(),
        repo_root: std::path::PathBuf::from("/tmp"),
        owner: "owner".into(),
        name: "repo".into(),
        default_branch: "main".into(),
        repo_description: None,
        pushed_at: None,
        is_private: None,
    };

    cleanup_stale_repo_file_urls(&ctx, &stats, false, &HashSet::new())
        .await
        .expect("partial no-source cleanup skip succeeds");

    stats.record_failed_file_read();
    cleanup_stale_repo_file_urls(&ctx, &stats, true, &HashSet::new())
        .await
        .expect("failed run cleanup skip succeeds");
}
