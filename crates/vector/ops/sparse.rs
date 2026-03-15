//! BM42-style sparse vector computation.
//!
//! Computes TF-weighted sparse vectors for Qdrant's `bm42` sparse index.
//! Qdrant applies IDF correction server-side (`"modifier": "idf"` in collection config).
//! Client-side we emit raw term frequency counts — no normalization needed here.
//!
//! # Hash stability
//! `term_to_index` uses a fixed-seed FNV-1a hash. The seed is baked into the constant
//! so the same term always maps to the same bucket index across process restarts.
//!
//! # Collision characteristics
//! With `SPARSE_DIM = 30_522` buckets and FNV-1a hashing, the birthday paradox gives
//! approximately 15% collision probability for 100 unique terms, 48% for 200 terms.
//! For typical web documents this means some distinct terms will share a bucket and
//! have their TF counts summed. This is a deliberate trade-off: using the actual BERT
//! tokenizer vocabulary would eliminate collisions but requires an external dependency.
//! Qdrant's IDF weighting mitigates the impact — high-IDF terms are unlikely to collide.

use crate::crates::core::logging::log_debug;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Number of sparse vector buckets. Matches BERT vocabulary size for compatibility.
pub const SPARSE_DIM: u32 = 30_522;

static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "the", "and", "for", "with", "that", "this", "from", "into", "how", "what", "where",
        "when", "you", "your", "are", "can", "does", "use", "using", "used", "get", "set", "via",
        "not", "all", "any", "but", "too", "out", "our", "their", "them", "they", "its", "then",
        "than", "also", "have", "has", "had", "was", "were", "who", "why",
    ]
    .into_iter()
    .collect()
});

/// Represents a Qdrant sparse vector as parallel `indices` and `values` arrays.
///
/// - `indices`: unique bucket indices in range `0..SPARSE_DIM`
/// - `values`: TF weight for each index (always positive; Qdrant applies IDF)
#[derive(Debug, Clone, Default)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

impl SparseVector {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Serialize to the Qdrant wire format expected in `"vector"` and `"query"` fields.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "indices": self.indices,
            "values": self.values,
        })
    }
}

/// Map a single lowercase alphanumeric term to a Qdrant bucket index.
///
/// Uses FNV-1a with a fixed seed so the mapping is stable across runs.
/// Exposed as `pub` so tests can verify specific terms are excluded.
pub fn term_to_index(term: &str) -> u32 {
    const FNV_OFFSET: u32 = 2_166_136_261;
    const FNV_PRIME: u32 = 16_777_619;
    let mut hash = FNV_OFFSET;
    for byte in term.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash % SPARSE_DIM
}

/// Compute a BM42-style sparse vector for `text`.
///
/// Returns a `SparseVector` with one entry per unique token bucket.
/// Short tokens (< 3 bytes; equivalent to < 3 chars for ASCII-only alphanumeric tokens),
/// stopwords, and non-alphanumeric tokens are excluded.
/// On hash collision two distinct terms map to the same bucket; their TF counts are summed.
pub fn compute_sparse_vector(text: &str) -> SparseVector {
    let mut bucket_tf: HashMap<u32, u32> = HashMap::new();
    for term in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let lower = term.to_ascii_lowercase();
        if lower.len() < 3 || STOP_WORDS.contains(lower.as_str()) {
            continue;
        }
        let idx = term_to_index(&lower);
        *bucket_tf.entry(idx).or_insert(0) += 1;
    }
    if bucket_tf.is_empty() {
        log_debug(&format!(
            "compute_sparse_vector: no indexable terms (len={}) — hybrid search will use dense-only",
            text.len()
        ));
        return SparseVector::default();
    }
    let mut indices = Vec::with_capacity(bucket_tf.len());
    let mut values = Vec::with_capacity(bucket_tf.len());
    for (idx, count) in bucket_tf {
        indices.push(idx);
        values.push(count as f32); // raw TF; Qdrant applies IDF server-side (see collision note in module doc)
    }
    SparseVector { indices, values }
}

#[cfg(test)]
mod tests {
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
}
