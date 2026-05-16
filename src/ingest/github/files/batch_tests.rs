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
    assert!(stats.has_failed_batches());
}
