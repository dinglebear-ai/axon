use super::indexed_token_stats_from_totals;
use std::collections::HashMap;

#[test]
fn indexed_token_stats_average_chunks_and_docs() {
    let doc_chars = HashMap::from([
        ("https://a.example".to_string(), 4_000usize),
        ("https://b.example".to_string(), 2_000usize),
    ]);
    let stats = indexed_token_stats_from_totals(4, 6_000, doc_chars, 5_000).unwrap();
    assert_eq!(stats.sampled_points, 4);
    assert_eq!(stats.sampled_docs, 2);
    assert_eq!(stats.avg_chunk_chars, 1_500.0);
    assert_eq!(stats.avg_chunk_tokens_estimate, 375.0);
    assert_eq!(stats.avg_doc_chars, 3_000.0);
    assert_eq!(stats.avg_doc_tokens_estimate, 750.0);
}

#[test]
fn indexed_token_stats_absent_without_samples() {
    assert!(indexed_token_stats_from_totals(0, 0, HashMap::new(), 5_000).is_none());
}
