//! Old-generation pruning policy.
//!
//! When a source is re-acquired, older generations become prunable. This module
//! decides *which* generations may be pruned, enforcing the generation-fence:
//! the current committed generation is never selected.
//!
//! See `docs/pipeline-unification/runtime/pruning-contract.md` (generation-fenced
//! deletes).

use axon_api::source::ids::SourceGenerationId;

use crate::safety::fence_generation;

/// Select the generations eligible for pruning: everything in `all` except the
/// `current` committed generation. Order is preserved. The current generation
/// is fenced out even if it appears multiple times.
pub fn prunable_generations(
    all: &[SourceGenerationId],
    current: &SourceGenerationId,
) -> Vec<SourceGenerationId> {
    all.iter()
        .filter(|g| fence_generation(g, current).is_ok())
        .cloned()
        .collect()
}

/// A retention policy that keeps the newest `keep` generations and prunes the
/// rest, always fencing out `current`. `generations` is expected
/// newest-first; the current generation is never pruned regardless of `keep`.
pub fn prune_beyond_retention(
    generations: &[SourceGenerationId],
    current: &SourceGenerationId,
    keep: usize,
) -> Vec<SourceGenerationId> {
    generations
        .iter()
        .skip(keep)
        .filter(|g| fence_generation(g, current).is_ok())
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
