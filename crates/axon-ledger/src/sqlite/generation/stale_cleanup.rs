use std::collections::BTreeMap;

use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::util::{json_error, manifest_item_changed, timestamp};
use crate::store::Result;

pub(super) async fn stale_item_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };
    let previous_items =
        manifest_items_in_tx(tx, &generation.source_id, previous_generation).await?;
    let next_items =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation).await?;
    let next_by_key = next_items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect::<BTreeMap<_, _>>();

    let mut cleanup_debt = Vec::new();
    for item in previous_items {
        if let Some(next) = next_by_key.get(&item.source_item_key)
            && !manifest_item_changed(&item, next)
        {
            continue;
        }
        let debt = CleanupDebt {
            debt_id: CleanupDebtId::new(format!(
                "debt_{}",
                uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_URL,
                    format!(
                        "{}:{}:{}",
                        generation.source_id.0, previous_generation.0, item.source_item_key.0
                    )
                    .as_bytes(),
                )
            )),
            job_id: JobId::new(uuid::Uuid::from_u128(0)),
            source_id: generation.source_id.clone(),
            generation: Some(previous_generation.clone()),
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::SourceItem {
                source_id: generation.source_id.clone(),
                source_item_key: item.source_item_key,
                generation: previous_generation.clone(),
            },
            status: LifecycleStatus::Pending,
            created_at: timestamp(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        };
        cleanup_debt.push(debt);
    }
    Ok(cleanup_debt)
}

async fn manifest_items_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Vec<ManifestItem>> {
    let rows = sqlx::query(
        r#"
        SELECT item_json
        FROM source_items
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(sqlite_error)?;

    rows.into_iter()
        .map(|row| {
            let item_json: String = row.get("item_json");
            serde_json::from_str(&item_json).map_err(json_error)
        })
        .collect()
}
