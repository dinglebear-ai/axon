//! Deterministic evidence diversity after committed-generation reconciliation.

use std::collections::HashMap;
use std::hash::Hash;

pub const MODULE_NAME: &str = "rank";

pub(crate) const DIVERSITY_WINDOW_SIZE: usize = 10;

/// Diversify each fixed ten-result window without moving evidence across its
/// boundary. Two complementary chunks per document lead each window; deferred
/// chunks retain provider order. This prevents duplicate crowding without
/// losing any evidence from a ten-result page. Scores and citations are intact.
pub(crate) fn diversify_documents<T, K: Eq + Hash>(
    hits: &mut Vec<T>,
    document_key: impl Fn(&T) -> K,
) {
    let candidates = std::mem::take(hits);
    let mut candidates = candidates.into_iter();
    let mut ranked = Vec::with_capacity(candidates.len());
    while candidates.len() > 0 {
        let mut counts = HashMap::new();
        let mut deferred = Vec::new();
        for hit in candidates.by_ref().take(DIVERSITY_WINDOW_SIZE) {
            let count = counts.entry(document_key(&hit)).or_insert(0_usize);
            *count += 1;
            if *count <= 2 {
                ranked.push(hit);
            } else {
                deferred.push(hit);
            }
        }
        ranked.extend(deferred);
    }
    *hits = ranked;
}

#[cfg(test)]
#[path = "rank_tests.rs"]
mod tests;
