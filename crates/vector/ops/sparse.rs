//! BM42-style sparse vector computation.
//!
//! Computes TF-weighted sparse vectors for Qdrant's `bm42` sparse index.
//! Qdrant applies IDF correction server-side (`"modifier": "idf"` in collection config).
//! Client-side we emit log-normalized term frequency: `ln(1 + raw_count)`.
//!
//! Log normalization prevents documents with extreme term repetition (e.g. 500 occurrences
//! of a term) from drowning out documents with moderate repetition (e.g. 20 occurrences).
//! This mirrors standard BM25 TF saturation: the marginal value of the 500th occurrence
//! is near zero. Qdrant's IDF weighting then amplifies rare, discriminative terms as usual.
//!
//! # Hash stability
//! `term_to_index` uses a fixed-seed FNV-1a hash. The seed is baked into the constant
//! so the same term always maps to the same bucket index across process restarts.
//!
//! # Collision characteristics
//! With `SPARSE_DIM = 65_536` buckets and FNV-1a hashing, the birthday paradox gives
//! approximately 12% collision probability for 100 unique terms, 24% for 200 terms —
//! half the collision rate of the original 30,522-bucket configuration.
//! No memory overhead: sparse vectors store only non-zero (index, value) pairs.

use crate::crates::core::logging::log_debug;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Number of sparse vector buckets.
///
/// Set to 65,536 (2^16) — double the original BERT vocabulary size (30,522).
/// The birthday paradox gives approximately 24% collision probability for 200 unique
/// terms at this bucket count, vs 48% at 30,522. The memory overhead is zero:
/// sparse vectors only store non-zero entries.
///
/// **Migration note:** Changing this constant makes existing sparse vectors (encoded
/// with 30,522 buckets) incompatible with new ones. When deploying, re-index all
/// content into a new named collection (`cortex_v2`) via `axon migrate` before
/// flipping `AXON_COLLECTION`. Do not apply this change to a live collection in place.
pub const SPARSE_DIM: u32 = 65_536;

/// Shared stop word set used by both sparse vector computation and BM25-style ranking.
///
/// Structural/syntactic words only. Content verbs like "make", "create", "build"
/// encode user intent and must NOT be stripped — they distinguish "how to USE a
/// library" from "how to IMPLEMENT an interface."
///
/// Extended from TS counterpart: high-frequency doc words that add noise without
/// distinguishing what a page is actually about.
pub(crate) static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "the", "and", "for", "with", "that", "this", "from", "into", "how", "what", "where",
        "when", "you", "your", "are", "can", "does", "via", "not", "all", "any", "but", "too",
        "out", "our", "their", "them", "they", "its", "then", "than", "also", "have", "has", "had",
        "was", "were", "who", "why",
    ]
    .into_iter()
    .collect()
});

/// Represents a Qdrant sparse vector as parallel `indices` and `values` arrays.
///
/// - `indices`: unique bucket indices in range `0..SPARSE_DIM`
/// - `values`: log-normalized TF weight `ln(1 + raw_count)` for each index (always positive; Qdrant applies IDF)
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
/// On hash collision two distinct terms map to the same bucket; their TF counts are summed
/// before log normalization.
///
/// TF weight = `ln(1 + raw_count)` — log normalization prevents high-repetition documents
/// from dominating BM42 scoring regardless of term content.
pub fn compute_sparse_vector(text: &str) -> SparseVector {
    // Pre-allocate for typical chunk sizes (~150 unique terms) to avoid 3-4 resizes.
    let estimated_capacity = if text.len() > 200 { 128 } else { 16 };
    let mut bucket_tf: HashMap<u32, u32> = HashMap::with_capacity(estimated_capacity);
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
        values.push((1.0_f32 + count as f32).ln()); // log-normalized TF; Qdrant applies IDF server-side
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
}
