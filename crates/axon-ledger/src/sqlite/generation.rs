use std::collections::BTreeSet;

use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::cleanup::insert_cleanup_debt_once_in_tx;
use crate::sqlite::util::{enum_wire_value, json_error, timestamp};
use crate::store::Result;

pub(super) async fn create_generation(
    store: &SqliteLedgerStore,
    source_id: SourceId,
) -> Result<SourceGeneration> {
    let previous_generation = committed_generation(store, &source_id).await?;
    let next_sequence: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(sequence), 0) + 1
        FROM source_generations
        WHERE source_id = ?1
        "#,
    )
    .bind(&source_id.0)
    .fetch_one(&store.pool)
    .await
    .map_err(sqlite_error)?;
    let generation = SourceGeneration {
        source_id: source_id.clone(),
        generation: SourceGenerationId::new(format!("gen_{next_sequence}")),
        status: LifecycleStatus::Running,
        publish_state: PublishState::Writing,
        created_at: timestamp(),
        published_at: None,
        item_counts: ItemCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: 0,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation,
    };
    upsert_generation(store, &generation, next_sequence).await?;
    Ok(generation)
}

pub(super) async fn publish_generation(
    store: &SqliteLedgerStore,
    generation: SourceGeneration,
) -> Result<()> {
    if !matches!(
        generation.status,
        LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded
    ) {
        return Err(ApiError::new(
            "source.ledger.generation_not_publishable",
            ErrorStage::Publishing,
            format!(
                "generation {} has non-publishable status {:?}",
                generation.generation.0, generation.status
            ),
        )
        .with_source_id(generation.source_id.0));
    }

    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    let manifest_exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM source_manifests
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&generation.source_id.0)
    .bind(&generation.generation.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    if manifest_exists.is_none() {
        return Err(ApiError::new(
            "source.ledger.manifest_missing",
            ErrorStage::Publishing,
            format!(
                "generation {} cannot publish without a manifest",
                generation.generation.0
            ),
        )
        .with_source_id(generation.source_id.0));
    }

    let previous = current_committed_generation_in_tx(&mut tx, &generation.source_id).await?;
    if previous != generation.previous_generation {
        return Err(ApiError::new(
            "source.ledger.generation_baseline_changed",
            ErrorStage::Publishing,
            format!(
                "generation {} was based on {:?}, but committed generation is {:?}",
                generation.generation.0, generation.previous_generation, previous
            ),
        )
        .with_source_id(generation.source_id.0));
    }

    let mut committed_generation = generation.clone();
    committed_generation.publish_state = PublishState::Committed;
    committed_generation.published_at = Some(timestamp());
    upsert_generation_in_tx(&mut tx, &committed_generation, None).await?;
    record_removed_item_cleanup_debt_in_tx(&mut tx, &committed_generation, previous.as_ref())
        .await?;

    let result = sqlx::query(
        r#"
        UPDATE sources
        SET committed_generation = ?1,
            updated_at = ?2
        WHERE source_id = ?3
          AND (
            (committed_generation IS NULL AND ?4 IS NULL)
            OR committed_generation = ?4
          )
        "#,
    )
    .bind(&committed_generation.generation.0)
    .bind(timestamp().0)
    .bind(&committed_generation.source_id.0)
    .bind(previous.as_ref().map(|value| value.0.as_str()))
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    if result.rows_affected() != 1 {
        return Err(ApiError::new(
            "source.ledger.generation_baseline_changed",
            ErrorStage::Publishing,
            format!(
                "source {} committed generation changed during publish",
                committed_generation.source_id.0
            ),
        )
        .with_source_id(committed_generation.source_id.0));
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn committed_generation(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
) -> Result<Option<SourceGenerationId>> {
    let committed_generation: Option<String> = sqlx::query_scalar(
        r#"
        SELECT committed_generation
        FROM sources
        WHERE source_id = ?1
        "#,
    )
    .bind(&source_id.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?
    .flatten();
    Ok(committed_generation.map(SourceGenerationId::new))
}

pub(super) async fn upsert_generation(
    store: &SqliteLedgerStore,
    generation: &SourceGeneration,
    sequence: i64,
) -> Result<()> {
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    upsert_generation_in_tx(&mut tx, generation, Some(sequence)).await?;
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn upsert_generation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    sequence: Option<i64>,
) -> Result<()> {
    let sequence = match sequence {
        Some(sequence) => sequence,
        None => sqlx::query_scalar(
            r#"
            SELECT sequence
            FROM source_generations
            WHERE source_id = ?1 AND generation = ?2
            "#,
        )
        .bind(&generation.source_id.0)
        .bind(&generation.generation.0)
        .fetch_one(&mut **tx)
        .await
        .map_err(sqlite_error)?,
    };
    let generation_json = serde_json::to_string(generation).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO source_generations (
            source_id,
            generation,
            sequence,
            status,
            publish_state,
            generation_json,
            created_at,
            published_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(source_id, generation) DO UPDATE SET
            sequence = COALESCE(excluded.sequence, source_generations.sequence),
            status = excluded.status,
            publish_state = excluded.publish_state,
            generation_json = excluded.generation_json,
            published_at = excluded.published_at
        "#,
    )
    .bind(&generation.source_id.0)
    .bind(&generation.generation.0)
    .bind(sequence)
    .bind(enum_wire_value(generation.status)?)
    .bind(enum_wire_value(generation.publish_state)?)
    .bind(generation_json)
    .bind(&generation.created_at.0)
    .bind(
        generation
            .published_at
            .as_ref()
            .map(|value| value.0.as_str()),
    )
    .execute(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn ensure_generation_for_manifest_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manifest: &SourceManifest,
) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM source_generations
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&manifest.source_id.0)
    .bind(&manifest.generation.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    if exists.is_some() {
        return Ok(());
    }

    let sequence: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(sequence), 0) + 1
        FROM source_generations
        WHERE source_id = ?1
        "#,
    )
    .bind(&manifest.source_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    let generation = SourceGeneration {
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        status: LifecycleStatus::Running,
        publish_state: PublishState::Writing,
        created_at: manifest.created_at.clone(),
        published_at: None,
        item_counts: ItemCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: manifest.items.len() as u64,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation: current_committed_generation_in_tx(tx, &manifest.source_id).await?,
    };
    upsert_generation_in_tx(tx, &generation, Some(sequence)).await
}

pub(super) async fn current_committed_generation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
) -> Result<Option<SourceGenerationId>> {
    let committed_generation: Option<String> = sqlx::query_scalar(
        r#"
        SELECT committed_generation
        FROM sources
        WHERE source_id = ?1
        "#,
    )
    .bind(&source_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?
    .flatten();
    Ok(committed_generation.map(SourceGenerationId::new))
}

async fn record_removed_item_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<()> {
    let Some(previous_generation) = previous_generation else {
        return Ok(());
    };
    let previous_items =
        manifest_items_in_tx(tx, &generation.source_id, previous_generation).await?;
    let next_items =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation).await?;
    let next_keys = next_items
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect::<BTreeSet<_>>();

    for item in previous_items {
        if next_keys.contains(&item.source_item_key) {
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
        insert_cleanup_debt_once_in_tx(tx, debt).await?;
    }
    Ok(())
}

pub(super) async fn manifest_items_in_tx(
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
