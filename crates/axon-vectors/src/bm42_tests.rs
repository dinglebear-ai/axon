use super::*;

fn cid(s: &str) -> ChunkId {
    ChunkId(s.to_string())
}

#[test]
fn computes_nonempty_sparse_for_real_text() {
    let sv = compute_bm42_sparse(cid("c1"), "unified pipeline data plane vector store");
    assert_eq!(sv.chunk_id.0, "c1");
    assert!(!sv.indices.is_empty());
    assert_eq!(sv.indices.len(), sv.values.len());
    // all values are log-normalized TF -> strictly positive.
    assert!(sv.values.iter().all(|v| *v > 0.0 && v.is_finite()));
    // all indices within the bucket space.
    assert!(sv.indices.iter().all(|i| *i < SPARSE_DIM));
}

#[test]
fn deterministic_same_text_same_buckets() {
    let a = compute_bm42_sparse(cid("c"), "hybrid retrieval with bm42 sparse vectors");
    let b = compute_bm42_sparse(cid("c"), "hybrid retrieval with bm42 sparse vectors");
    let mut ai: Vec<_> = a.indices.clone();
    let mut bi: Vec<_> = b.indices.clone();
    ai.sort_unstable();
    bi.sort_unstable();
    assert_eq!(ai, bi);
}

#[test]
fn stopwords_and_short_tokens_excluded() {
    // "the", "and", "of", "to", "is", "a" are stop words; "in" too; "xy" < 3.
    let sv = compute_bm42_sparse(cid("c"), "the and of to is a in xy");
    assert!(
        sv.indices.is_empty(),
        "all-stopword/short input must yield an empty sparse vector, got {:?}",
        sv.indices
    );
}

#[test]
fn repeated_term_raises_tf_weight() {
    let once = compute_bm42_sparse(cid("c"), "kubernetes");
    let thrice = compute_bm42_sparse(cid("c"), "kubernetes kubernetes kubernetes");
    assert_eq!(once.indices, thrice.indices);
    assert!(
        thrice.values[0] > once.values[0],
        "ln(1+3) must exceed ln(1+1)"
    );
}
