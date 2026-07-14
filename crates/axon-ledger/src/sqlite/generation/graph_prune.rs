//! `GraphPrune` cleanup-debt production for genuinely removed manifest items.
//!
//! Unlike `VectorDelete` (which must fire for both modified *and* removed
//! items, since vector points are always regenerated per-generation), a graph
//! node's `stable_key` is identity, not generation-scoped — the baseline
//! document node for an item is keyed by the item's own `source_item_key`
//! (see `crates/axon-services/src/source/graph.rs::document_stable_key`). A
//! merely *modified* item keeps the same key into the new generation, so its
//! graph node must stay; only an item entirely absent from the new manifest
//! should have its graph node pruned.
//!
//! This only prunes the conservative, deterministically-known identity: the
//! removed item's own document-node stable key. Parser-produced graph nodes
//! (extra candidates from `enriching`) are not derivable here and are left
//! alone — see the module-level note in `docs/pipeline-unification/runtime/
//! pruning-contract.md` ("graph orphan cleanup").

use std::collections::BTreeSet;

use axon_api::source::*;

use crate::cleanup_debt::graph_prune_debt;
use crate::store::Result;

use super::manifest_items::manifest_items_in_tx;

pub(super) async fn graph_prune_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };
    let previous_items =
        manifest_items_in_tx(tx, &generation.source_id, previous_generation).await?;
    let next_keys: BTreeSet<SourceItemKey> =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation)
            .await?
            .into_iter()
            .map(|item| item.source_item_key)
            .collect();

    let mut cleanup_debt = Vec::new();
    for item in previous_items {
        if next_keys.contains(&item.source_item_key) {
            // Still present (unchanged or modified) — same stable key, same
            // graph node. Only a true removal prunes the node.
            continue;
        }
        cleanup_debt.push(graph_prune_debt(
            &generation.source_id,
            previous_generation,
            &item.source_item_key,
        ));
    }
    Ok(cleanup_debt)
}
