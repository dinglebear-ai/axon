//! Dedupe policy — a prune with a non-source selector.
//!
//! Contract (§"Dedupe"): dedupe must compute duplicate candidates, preserve the
//! best point/chunk, produce a dry-run report, and delete only the selected
//! duplicate vector points. This module owns the pure candidate/keep/delete
//! decision; the executor performs the actual deletes.

use std::collections::BTreeMap;

use axon_api::source::ids::VectorPointId;

/// A single vector point considered for dedupe.
#[derive(Debug, Clone, PartialEq)]
pub struct DedupeCandidate {
    pub point_id: VectorPointId,
    /// Content-identity key: points sharing this are near-duplicates.
    pub dup_key: String,
    /// Quality score; the highest-scoring point in a group is preserved.
    pub score: f32,
}

/// A dry-run dedupe plan: which points to keep and which to delete.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DedupePlan {
    /// Points preserved (one best per duplicate group).
    pub kept: Vec<VectorPointId>,
    /// Points selected for deletion (all non-best duplicates).
    pub to_delete: Vec<VectorPointId>,
}

impl DedupePlan {
    pub fn delete_count(&self) -> usize {
        self.to_delete.len()
    }
}

/// Group candidates by `dup_key`, preserve the highest-scoring point per group
/// (ties broken by point id for determinism), and select the rest for
/// deletion. Singleton groups keep their only point and delete nothing.
pub fn plan_dedupe(candidates: &[DedupeCandidate]) -> DedupePlan {
    let mut groups: BTreeMap<&str, Vec<&DedupeCandidate>> = BTreeMap::new();
    for c in candidates {
        groups.entry(c.dup_key.as_str()).or_default().push(c);
    }

    let mut plan = DedupePlan::default();
    for (_key, mut members) in groups {
        // Best = highest score; tie-break by point id ascending for stability.
        members.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.point_id.0.cmp(&b.point_id.0))
        });
        let (best, rest) = members.split_first().expect("group is non-empty");
        plan.kept.push(best.point_id.clone());
        for dup in rest {
            plan.to_delete.push(dup.point_id.clone());
        }
    }

    plan.kept.sort_by(|a, b| a.0.cmp(&b.0));
    plan.to_delete.sort_by(|a, b| a.0.cmp(&b.0));
    plan
}

#[cfg(test)]
#[path = "dedupe_tests.rs"]
mod tests;
