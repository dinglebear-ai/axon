use super::*;

#[test]
fn ordinary_dense_search_uses_ann() {
    let body = named_dense_body(&[0.1, 0.2], "dense", 5, None);
    assert_eq!(body["params"]["exact"], false);
    assert_eq!(body["params"]["hnsw_ef"], HNSW_EF_SEARCH);
}

#[test]
fn hybrid_dense_prefetch_uses_ann() {
    let sparse = SparseVector {
        chunk_id: ChunkId::new("chunk-1"),
        indices: vec![1],
        values: vec![0.5],
    };
    let body = hybrid_body(&[0.1, 0.2], "dense", &sparse, "bm42", 5, None);
    assert_eq!(body["prefetch"][0]["params"]["exact"], false);
    assert_eq!(body["prefetch"][0]["params"]["hnsw_ef"], HNSW_EF_SEARCH);
}
