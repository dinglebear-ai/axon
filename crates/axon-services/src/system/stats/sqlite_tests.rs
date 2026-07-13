use super::{estimated_avg_chunk_tokens, estimated_avg_doc_tokens};

#[test]
fn estimates_token_averages_from_chunk_and_doc_totals() {
    assert_eq!(estimated_avg_chunk_tokens(), 500.0);
    assert_eq!(estimated_avg_doc_tokens(Some(4), Some(40)), Some(5_000.0));
}

#[test]
fn token_average_is_absent_without_docs() {
    assert_eq!(estimated_avg_doc_tokens(Some(0), Some(40)), None);
    assert_eq!(estimated_avg_doc_tokens(None, Some(40)), None);
}
