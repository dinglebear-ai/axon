use super::*;

#[test]
fn compute_sparse_vector_empty_text_returns_empty() {
    let sv = compute_sparse_vector("");
    assert!(sv.indices.is_empty());
    assert!(sv.values.is_empty());
}

#[test]
fn compute_sparse_vector_whitespace_only_returns_empty() {
    let sv = compute_sparse_vector("   \n\t  ");
    assert!(sv.indices.is_empty());
}

#[test]
fn compute_sparse_vector_indices_and_values_same_length() {
    let sv = compute_sparse_vector("hello world rust programming");
    assert_eq!(sv.indices.len(), sv.values.len());
}

#[test]
fn compute_sparse_vector_all_values_positive() {
    let sv = compute_sparse_vector("axon collection config embed");
    for &v in &sv.values {
        assert!(v > 0.0, "all TF weights must be positive, got {v}");
    }
}

#[test]
fn compute_sparse_vector_all_indices_in_bucket_range() {
    let sv = compute_sparse_vector("qdrant vector sparse embedding search");
    for &idx in &sv.indices {
        assert!(
            idx < SPARSE_DIM,
            "index {idx} must be < SPARSE_DIM={SPARSE_DIM}"
        );
    }
}

#[test]
fn compute_sparse_vector_repeated_term_has_higher_weight() {
    let sv_single = compute_sparse_vector("rust language systems");
    let sv_triple = compute_sparse_vector("rust rust rust language systems");
    let rust_idx = term_to_index("rust");
    let single_val = sv_single
        .indices
        .iter()
        .zip(&sv_single.values)
        .find(|&(&i, _)| i == rust_idx)
        .map(|(_, &v)| v)
        .unwrap_or(0.0);
    let triple_val = sv_triple
        .indices
        .iter()
        .zip(&sv_triple.values)
        .find(|&(&i, _)| i == rust_idx)
        .map(|(_, &v)| v)
        .unwrap_or(0.0);
    assert!(
        triple_val > single_val,
        "triple occurrence must have higher weight: {triple_val} <= {single_val}"
    );
}

#[test]
fn compute_sparse_vector_stopwords_excluded() {
    let sv = compute_sparse_vector("the quick and brown fox");
    let the_idx = term_to_index("the");
    let and_idx = term_to_index("and");
    assert!(
        !sv.indices.contains(&the_idx),
        "stopword 'the' must not appear"
    );
    assert!(
        !sv.indices.contains(&and_idx),
        "stopword 'and' must not appear"
    );
}

#[test]
fn compute_sparse_vector_no_duplicate_indices() {
    let sv = compute_sparse_vector("embed qdrant vector search collection cortex");
    let mut seen = HashSet::new();
    for &idx in &sv.indices {
        assert!(
            seen.insert(idx),
            "duplicate index {idx} found in sparse vector"
        );
    }
}

#[test]
fn term_to_index_is_stable() {
    let idx_a = term_to_index("embedding");
    let idx_b = term_to_index("embedding");
    assert_eq!(idx_a, idx_b, "term_to_index must be deterministic");
}

#[test]
fn compute_sparse_vector_tech_terms_not_stopwords() {
    // "use", "using", "used", "get", "set" are domain-significant in tech docs
    // and must NOT be filtered as stopwords.
    let sv = compute_sparse_vector("use the api to get and set values using rust used before");
    let use_idx = term_to_index("use");
    let using_idx = term_to_index("using");
    let used_idx = term_to_index("used");
    let get_idx = term_to_index("get");
    let set_idx = term_to_index("set");
    assert!(
        sv.indices.contains(&use_idx),
        "tech term 'use' must not be a stopword"
    );
    assert!(
        sv.indices.contains(&using_idx),
        "tech term 'using' must not be a stopword"
    );
    assert!(
        sv.indices.contains(&used_idx),
        "tech term 'used' must not be a stopword"
    );
    assert!(
        sv.indices.contains(&get_idx),
        "tech term 'get' must not be a stopword"
    );
    assert!(
        sv.indices.contains(&set_idx),
        "tech term 'set' must not be a stopword"
    );
}

#[test]
fn compute_sparse_vector_short_tokens_excluded() {
    let sv = compute_sparse_vector("go is ok but rust is great");
    let go_idx = term_to_index("go");
    let is_idx = term_to_index("is");
    assert!(
        !sv.indices.contains(&go_idx),
        "short token 'go' must not appear"
    );
    assert!(
        !sv.indices.contains(&is_idx),
        "short token 'is' must not appear"
    );
}

#[test]
fn sparse_dim_is_65536() {
    assert_eq!(
        SPARSE_DIM, 65_536,
        "SPARSE_DIM must be 65536 to halve collision probability vs the old 30522"
    );
}
