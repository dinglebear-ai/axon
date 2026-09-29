//! Retire graph contributions for absent items and persisted content skips.
use std::collections::BTreeSet;

use axon_api::source::*;

use crate::cleanup_debt::graph_prune_debt;
use crate::store::Result;

use super::manifest_items::manifest_items_in_tx;

pub(super) async fn graph_prune_cleanup_debt_in_tx(
    tx: &mut sqlx::SqliteConnection,
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

    let skipped: BTreeSet<String> = sqlx::query_scalar("SELECT source_item_key FROM document_status WHERE source_id = ?1 AND generation = ?2 AND status = 'skipped'")
        .bind(&generation.source_id.0).bind(&generation.generation.0)
        .fetch_all(&mut *tx).await.map_err(crate::migration::sqlite_error)?.into_iter().collect();
    let mut cleanup_debt = Vec::new();
    for item in previous_items {
        if next_keys.contains(&item.source_item_key) && !skipped.contains(&item.source_item_key.0) {
            // Supported present items retain their graph contributions.
            continue;
        }
        cleanup_debt.push(graph_prune_debt(
            &generation.source_id,
            previous_generation,
            &item.source_item_key,
            &generation.generation,
        ));
    }
    Ok(cleanup_debt)
}
