use std::collections::{BTreeMap, BTreeSet};

use axon_api::source::*;

use crate::cleanup_debt::{
    artifact_delete_debt_for_metadata, cache_prune_debt_for_metadata, vector_delete_debt,
};
use crate::sqlite::util::{json_error, manifest_item_changed};
use crate::store::Result;

use super::manifest_items::{manifest_in_tx, manifest_items_in_tx};

pub(super) async fn stale_item_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };
    let previous_manifest = manifest_in_tx(tx, &generation.source_id, previous_generation).await?;
    let previous_items = previous_manifest
        .as_ref()
        .map(|manifest| manifest.items.clone())
        .unwrap_or_else(Vec::new);
    let next_items =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation).await?;
    let next_by_key = next_items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect::<BTreeMap<_, _>>();

    let mut cleanup_debt = Vec::new();
    let mut artifact_ids = BTreeSet::new();
    if let Some(previous_manifest) = previous_manifest.as_ref() {
        cleanup_debt.extend(
            artifact_delete_debt_for_metadata(
                &generation.source_id,
                previous_generation,
                &previous_manifest.metadata,
                "manifest",
                &mut artifact_ids,
            )
            .map_err(json_error)?,
        );
    }
    for item in previous_items {
        if let Some(next) = next_by_key.get(&item.source_item_key)
            && !manifest_item_changed(&item, next)
        {
            continue;
        }
        cleanup_debt.push(vector_delete_debt(
            &generation.source_id,
            previous_generation,
            &item.source_item_key,
        ));
        cleanup_debt.extend(
            artifact_delete_debt_for_metadata(
                &generation.source_id,
                previous_generation,
                &item.metadata,
                &item.source_item_key.0,
                &mut artifact_ids,
            )
            .map_err(json_error)?,
        );
        if let Some(debt) = cache_prune_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &item.metadata,
            &item.source_item_key,
        )
        .map_err(json_error)?
        {
            cleanup_debt.push(debt);
        }
    }
    Ok(cleanup_debt)
}
