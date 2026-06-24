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
        "a", "am", "an", "and", "any", "are", "as", "at", "be", "but", "by", "can", "do", "does",
        "for", "from", "had", "has", "have", "he", "her", "him", "his", "how", "if", "in", "into",
        "is", "it", "its", "me", "my", "no", "not", "of", "on", "or", "our", "out", "she", "so",
        "than", "that", "the", "their", "them", "then", "they", "this", "to", "too", "up", "us",
        "via", "was", "we", "were", "what", "when", "where", "who", "why", "you", "your",
    ]
    .into_iter()
    .collect()
});

/// Represents a Qdrant sparse vector as parallel `indices` and `values` arrays.
///
/// - `indices`: unique bucket indices in range `0..SPARSE_DIM`
/// - `values`: log-normalized TF weight `ln(1 + raw_count)` for each index (always positive; Qdrant applies IDF)
///
/// `Serialize` emits the Qdrant wire shape directly (`{indices: [...], values: [...]}`),
/// so call sites can embed `&sparse_vector` straight inside `serde_json::json!{...}`
/// without round-tripping through an intermediate `serde_json::Value`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

impl SparseVector {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Map an alphanumeric term to a Qdrant bucket index.
///
/// Uses FNV-1a with a fixed seed so the mapping is stable across runs.
/// Case is folded inside the hash loop so callers do not need to pre-lowercase
/// the term — `term_to_index("Rust")` and `term_to_index("rust")` produce the
/// same bucket. (P-L1)
/// Exposed as `pub` so tests can verify specific terms are excluded.
pub fn term_to_index(term: &str) -> u32 {
    const FNV_OFFSET: u32 = 2_166_136_261;
    const FNV_PRIME: u32 = 16_777_619;
    let mut hash = FNV_OFFSET;
    for byte in term.as_bytes() {
        hash ^= u32::from(byte.to_ascii_lowercase());
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
/// Hard cap on terms scanned per call. Defends against pathological inputs
/// where a multi-MB chunk slips past the dispatch query cap (chunks during
/// indexing are not query-capped) or where the query cap is raised without
/// re-evaluating sparse-vector cost. With ~150 unique terms typical, 65,536
/// is well above any real chunk and well below any DoS shape. (bd
/// axon_rust-d71.34 / M-SEC)
const MAX_TERMS_PER_VECTOR: usize = 65_536;

/// Longest stop word in `STOP_WORDS`, in bytes. Ties together the length guard
/// and the stack buffer below: any term longer than this cannot match a stop
/// word, and the lowercasing buffer is exactly this size — keep the two in sync.
const STOP_WORD_MAX_BYTES: usize = 5;

pub fn compute_sparse_vector(text: &str) -> SparseVector {
    compute_sparse_vector_inner(text, true)
}

/// Compute a sparse vector for document indexing.
///
/// Empty sparse vectors are expected for tiny DOM fragments such as punctuation
/// separators. Query-time callers use `compute_sparse_vector()` so operators
/// still see visible dense-only fallback warnings for user/search inputs.
pub fn compute_sparse_vector_for_indexing(text: &str) -> SparseVector {
    compute_sparse_vector_inner(text, false)
}

fn compute_sparse_vector_inner(text: &str, warn_on_empty: bool) -> SparseVector {
    // Pre-allocate for typical chunk sizes (~150 unique terms) to avoid 3-4 resizes.
    let estimated_capacity = if text.len() > 200 { 128 } else { 16 };
    let mut bucket_tf: HashMap<u32, u32> = HashMap::with_capacity(estimated_capacity);
    let mut scanned: usize = 0;
    for term in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if term.len() < 3 {
            continue;
        }
        // Allocation-free stop-word check. (P-L1)
        // All stop words are ASCII-lowercase and ≤ 5 bytes. Any term longer
        // than 5 bytes cannot match a stop word. For short terms we do a
        // direct HashSet lookup when the term is already lowercase (the common
        // case), and only build a tiny 5-byte stack copy otherwise.
        let is_stop_word = if term.len() > STOP_WORD_MAX_BYTES {
            false
        } else if term.bytes().all(|b| b.is_ascii_lowercase()) {
            STOP_WORDS.contains(term)
        } else {
            // term has uppercase — lowercase into a stack buffer, no heap alloc.
            let mut buf = [0u8; STOP_WORD_MAX_BYTES];
            for (i, b) in term.bytes().enumerate() {
                buf[i] = b.to_ascii_lowercase();
            }
            // SAFETY: buf[..n] contains only ASCII bytes from to_ascii_lowercase().
            let s = std::str::from_utf8(&buf[..term.len()]).unwrap_or("");
            STOP_WORDS.contains(s)
        };
        if is_stop_word {
            continue;
        }
        scanned += 1;
        if scanned > MAX_TERMS_PER_VECTOR {
            tracing::warn!(
                len = text.len(),
                cap = MAX_TERMS_PER_VECTOR,
                "compute_sparse_vector: term cap reached — truncating"
            );
            break;
        }
        // term_to_index folds case internally — pass raw term, no String needed. (P-L1)
        let idx = term_to_index(term);
        *bucket_tf.entry(idx).or_insert(0) += 1;
    }
    if bucket_tf.is_empty() {
        // Promoted from log_debug → tracing::warn! so default-INFO operators
        // see when hybrid search silently falls back to dense-only (typically
        // when the query is non-ASCII, all-stopwords, or every term is < 3
        // chars). Includes a coarse character profile so operators can spot
        // patterns. (bd axon_rust-d71.9 / H5)
        if warn_on_empty {
            let mut ascii_alnum = 0usize;
            let mut non_ascii = 0usize;
            let mut whitespace = 0usize;
            let mut other = 0usize;
            for c in text.chars() {
                if c.is_ascii_alphanumeric() {
                    ascii_alnum += 1;
                } else if !c.is_ascii() {
                    non_ascii += 1;
                } else if c.is_whitespace() {
                    whitespace += 1;
                } else {
                    other += 1;
                }
            }
            tracing::warn!(
                len = text.len(),
                ascii_alnum,
                non_ascii,
                whitespace,
                other,
                "compute_sparse_vector: no indexable terms — hybrid search will use dense-only"
            );
        }
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
#[path = "sparse_tests.rs"]
mod tests;
