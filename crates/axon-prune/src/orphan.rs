//! Vector/artifact orphan cleanup policy.
//!
//! An orphan is a stored item (vector point or artifact) whose owning
//! document/generation no longer exists in the ledger. This module computes the
//! orphan set from a "live" reference set and a "stored" set — pure set
//! difference, no store access — so the executor can delete exactly the
//! dangling items.

use std::collections::BTreeSet;

/// Compute orphaned ids: stored ids with no live owner. Deterministic order
/// (sorted). Inputs may contain duplicates; the result is de-duplicated.
pub fn orphaned_ids<T>(stored: &[T], live: &[T]) -> Vec<T>
where
    T: Ord + Clone,
{
    let live_set: BTreeSet<&T> = live.iter().collect();
    let mut orphans: BTreeSet<T> = BTreeSet::new();
    for id in stored {
        if !live_set.contains(id) {
            orphans.insert(id.clone());
        }
    }
    orphans.into_iter().collect()
}

/// Whether the orphan computation is a no-op (nothing stored is dangling).
pub fn has_orphans<T>(stored: &[T], live: &[T]) -> bool
where
    T: Ord + Clone,
{
    !orphaned_ids(stored, live).is_empty()
}

#[cfg(test)]
#[path = "orphan_tests.rs"]
mod tests;
