//! BM42-style sparse vector computation for hybrid (dense + sparse) retrieval.
//!
//! Computes TF-weighted sparse vectors for Qdrant's `bm42` sparse index; Qdrant
//! applies IDF correction server-side (`"modifier": "idf"` in the collection
//! config). Client-side we emit log-normalized term frequency `ln(1 + tf)`.
//!
//! This is a faithful port of the legacy `axon-vector::ops::sparse` algorithm
//! (fixed-seed FNV-1a `term_to_index`, `SPARSE_DIM = 65_536`, the shared stop
//! word set, `ln(1 + tf)` weighting). The port MUST stay bucket-for-bucket
//! identical to the query-time computation or hybrid RRF fuses mismatched
//! sparse arms — so the constants and tokenization here mirror that module
//! exactly. When retrieval fully moves onto this crate, the legacy copy is
//! removed and this becomes the single source.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use axon_api::source::{ChunkId, SparseVector};

/// Number of sparse vector buckets (2^16). Changing this makes existing sparse
/// vectors incompatible — must match the query-side and any migrated data.
pub const SPARSE_DIM: u32 = 65_536;

/// Longest stop word in [`STOP_WORDS`], in bytes — bounds the allocation-free
/// stop-word check below.
const STOP_WORD_MAX_BYTES: usize = 5;

/// Hard cap on terms scanned per vector; defends against pathological inputs.
const MAX_TERMS_PER_VECTOR: usize = 65_536;

/// Structural/syntactic stop words only. Content verbs ("make", "create",
/// "build") encode intent and are intentionally NOT stripped. Must match the
/// query-side set.
static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
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

/// Map an alphanumeric term to a Qdrant bucket index via fixed-seed FNV-1a,
/// case-folded. Stable across runs; must match the query-side mapping.
fn term_to_index(term: &str) -> u32 {
    const FNV_OFFSET: u32 = 2_166_136_261;
    const FNV_PRIME: u32 = 16_777_619;
    let mut hash = FNV_OFFSET;
    for byte in term.as_bytes() {
        hash ^= u32::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash % SPARSE_DIM
}

/// Compute the BM42 sparse vector for one chunk's `text`, tagged with `chunk_id`.
///
/// Returns an empty sparse vector (no indices) when the text has no indexable
/// terms (tiny fragments, all-stopwords, non-ASCII) — a dense-only point, which
/// hybrid RRF tolerates (its sparse arm simply contributes nothing).
pub fn compute_bm42_sparse(chunk_id: ChunkId, text: &str) -> SparseVector {
    let estimated_capacity = if text.len() > 200 { 128 } else { 16 };
    let mut bucket_tf: HashMap<u32, u32> = HashMap::with_capacity(estimated_capacity);
    let mut scanned: usize = 0;

    for term in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if term.len() < 3 {
            continue;
        }
        let is_stop_word = if term.len() > STOP_WORD_MAX_BYTES {
            false
        } else if term.bytes().all(|b| b.is_ascii_lowercase()) {
            STOP_WORDS.contains(term)
        } else {
            let mut buf = [0u8; STOP_WORD_MAX_BYTES];
            for (i, b) in term.bytes().enumerate() {
                buf[i] = b.to_ascii_lowercase();
            }
            let s = std::str::from_utf8(&buf[..term.len()]).unwrap_or("");
            STOP_WORDS.contains(s)
        };
        if is_stop_word {
            continue;
        }
        scanned += 1;
        if scanned > MAX_TERMS_PER_VECTOR {
            break;
        }
        *bucket_tf.entry(term_to_index(term)).or_insert(0) += 1;
    }

    let mut indices = Vec::with_capacity(bucket_tf.len());
    let mut values = Vec::with_capacity(bucket_tf.len());
    for (idx, count) in bucket_tf {
        indices.push(idx);
        values.push((1.0_f32 + count as f32).ln());
    }
    SparseVector {
        chunk_id,
        indices,
        values,
    }
}

#[cfg(test)]
#[path = "bm42_tests.rs"]
mod tests;
