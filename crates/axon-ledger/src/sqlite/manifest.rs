use std::collections::BTreeSet;

use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::generation::{committed_generation, ensure_generation_for_manifest_in_tx};
use crate::sqlite::util::{json_error, keyed_manifest_items, manifest_item_changed, stage_header};
use crate::store::Result;
use crate::validation::validate_manifest;

pub(super) async fn put_manifest(
    store: &SqliteLedgerStore,
    manifest: SourceManifest,
) -> Result<()> {
    validate_manifest(&manifest)?;
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    ensure_generation_for_manifest_in_tx(&mut tx, &manifest).await?;
    let manifest_json = serde_json::to_string(&manifest).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO source_manifests (
            source_id,
            generation,
            manifest_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(source_id, generation) DO UPDATE SET
            manifest_json = excluded.manifest_json,
            created_at = excluded.created_at
        "#,
    )
    .bind(&manifest.source_id.0)
    .bind(&manifest.generation.0)
    .bind(manifest_json)
    .bind(&manifest.created_at.0)
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;

    sqlx::query(
        r#"
        DELETE FROM source_items
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&manifest.source_id.0)
    .bind(&manifest.generation.0)
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;

    for item in &manifest.items {
        let item_json = serde_json::to_string(item).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT INTO source_items (
                source_id,
                source_item_key,
                generation,
                item_canonical_uri,
                content_hash,
                version,
                mtime,
                item_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&item.source_id.0)
        .bind(&item.source_item_key.0)
        .bind(&manifest.generation.0)
        .bind(&item.canonical_uri)
        .bind(item.content_hash.as_deref())
        .bind(item.version.as_deref())
        .bind(item.mtime.as_ref().map(|value| value.0.as_str()))
        .bind(item_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlite_error)?;
    }

    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn diff_manifest(
    store: &SqliteLedgerStore,
    manifest: SourceManifest,
) -> Result<SourceManifestDiff> {
    let previous_generation = committed_generation(store, &manifest.source_id).await?;
    let previous = match &previous_generation {
        Some(generation) => {
            let manifest = read_manifest(store, &manifest.source_id, generation)
                .await?
                .ok_or_else(|| {
                    ApiError::new(
                        "source.ledger.committed_manifest_missing",
                        ErrorStage::Diffing,
                        format!("committed manifest {} is missing", generation.0),
                    )
                    .with_source_id(manifest.source_id.0.clone())
                })?;
            keyed_manifest_items(manifest.items)
        }
        None => Default::default(),
    };
    let SourceManifest {
        source_id,
        generation,
        items,
        ..
    } = manifest;
    let next = keyed_manifest_items(items);

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut unchanged = Vec::new();
    for (key, item) in &next {
        match previous.get(key) {
            None => added.push(item.clone()),
            Some(old) if manifest_item_changed(old, item) => modified.push(item.clone()),
            Some(_) => unchanged.push(item.clone()),
        }
    }

    let next_keys = next.keys().cloned().collect::<BTreeSet<_>>();
    let removed = previous
        .into_iter()
        .filter_map(|(key, item)| (!next_keys.contains(&key)).then_some(item))
        .collect::<Vec<_>>();

    Ok(SourceManifestDiff {
        header: stage_header(PipelinePhase::Diffing),
        source_id,
        previous_generation,
        next_generation: generation,
        counts: DiffCounts {
            added: added.len() as u64,
            modified: modified.len() as u64,
            removed: removed.len() as u64,
            unchanged: unchanged.len() as u64,
            skipped: 0,
            failed: 0,
        },
        added,
        modified,
        removed,
        unchanged,
        skipped: Vec::new(),
        failed: Vec::new(),
    })
}

pub(super) async fn read_manifest(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Option<SourceManifest>> {
    let row = sqlx::query(
        r#"
        SELECT manifest_json
        FROM source_manifests
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;

    row.map(|row| {
        let manifest_json: String = row.get("manifest_json");
        serde_json::from_str(&manifest_json).map_err(json_error)
    })
    .transpose()
}
