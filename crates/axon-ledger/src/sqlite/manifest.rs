use std::collections::BTreeMap;

use axon_api::source::*;
use axon_core::sqlite::ImmediateTx;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::generation::{committed_generation, ensure_generation_for_manifest_in_tx};
use crate::sqlite::util::{json_error, stage_header};
use crate::store::Result;
use crate::validation::validate_manifest;

#[derive(serde::Serialize)]
struct ManifestHeader<'a> {
    source_id: &'a SourceId,
    generation: &'a SourceGenerationId,
    adapter: &'a AdapterRef,
    scope: &'a SourceScope,
    items: &'a [ManifestItem],
    created_at: &'a Timestamp,
    metadata: &'a MetadataMap,
}

fn serialize_manifest_header(manifest: &SourceManifest) -> Result<String> {
    serde_json::to_string(&ManifestHeader {
        source_id: &manifest.source_id,
        generation: &manifest.generation,
        adapter: &manifest.adapter,
        scope: &manifest.scope,
        items: &[],
        created_at: &manifest.created_at,
        metadata: &manifest.metadata,
    })
    .map_err(json_error)
}

pub(super) fn reconstruct_manifest(
    manifest_json: &str,
    normalized_items: Vec<ManifestItem>,
) -> Result<SourceManifest> {
    let mut manifest: SourceManifest = serde_json::from_str(manifest_json).map_err(json_error)?;
    if !normalized_items.is_empty() || manifest.items.is_empty() {
        manifest.items = normalized_items;
    }
    Ok(manifest)
}

pub(super) async fn put_manifest(
    store: &SqliteLedgerStore,
    manifest: &SourceManifest,
) -> Result<()> {
    validate_manifest(manifest)?;
    let mut tx = ImmediateTx::begin(&store.pool)
        .await
        .map_err(sqlite_error)?;
    ensure_generation_for_manifest_in_tx(&mut tx, manifest).await?;
    let manifest_json = serialize_manifest_header(manifest)?;
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

    // Eight bind parameters per item; batches of 100 stay below SQLite's
    // conservative 999-variable ceiling while collapsing a corpus-sized series
    // of individual INSERT executions into a handful of statements.
    const MANIFEST_ITEM_INSERT_BATCH_SIZE: usize = 100;
    for items in manifest.items.chunks(MANIFEST_ITEM_INSERT_BATCH_SIZE) {
        let item_json = items
            .iter()
            .map(|item| serde_json::to_string(item).map_err(json_error))
            .collect::<Result<Vec<_>>>()?;
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO source_items (source_id, source_item_key, generation,              item_canonical_uri, content_hash, version, mtime, item_json) ",
        );
        query.push_values(
            items.iter().zip(&item_json),
            |mut row, (item, item_json)| {
                row.push_bind(&item.source_id.0)
                    .push_bind(&item.source_item_key.0)
                    .push_bind(&manifest.generation.0)
                    .push_bind(&item.canonical_uri)
                    .push_bind(item.content_hash.as_deref())
                    .push_bind(item.version.as_deref())
                    .push_bind(item.mtime.as_ref().map(|value| value.0.as_str()))
                    .push_bind(item_json);
            },
        );
        query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(sqlite_error)?;
    }

    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn diff_manifest(
    store: &SqliteLedgerStore,
    manifest: &SourceManifest,
) -> Result<SourceManifestDiff> {
    let previous_generation = committed_generation(store, &manifest.source_id).await?;
    let mut previous = match &previous_generation {
        Some(generation) => {
            ensure_manifest_exists(store, &manifest.source_id, generation).await?;
            previous_fingerprints(store, &manifest.source_id, generation).await?
        }
        None => BTreeMap::new(),
    };

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut unchanged = Vec::new();
    for item in &manifest.items {
        match previous.remove(&item.source_item_key.0) {
            None => added.push(item.clone()),
            Some(old) if old.changed(item) => modified.push(item.clone()),
            Some(_) => unchanged.push(item.clone()),
        }
    }

    let removed = match &previous_generation {
        Some(generation) if !previous.is_empty() => {
            let keys = previous.keys().cloned().map(SourceItemKey::from).collect();
            read_manifest_items(store, &manifest.source_id, generation, keys).await?
        }
        _ => Vec::new(),
    };

    Ok(SourceManifestDiff {
        header: stage_header(PipelinePhase::Diffing),
        source_id: manifest.source_id.clone(),
        previous_generation,
        next_generation: manifest.generation.clone(),
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

#[derive(Debug)]
struct PreviousFingerprint {
    content_hash: Option<String>,
    version: Option<String>,
    mtime: Option<String>,
}

impl PreviousFingerprint {
    fn changed(&self, next: &ManifestItem) -> bool {
        self.content_hash.as_deref() != next.content_hash.as_deref()
            || self.version.as_deref() != next.version.as_deref()
            || self.mtime.as_deref() != next.mtime.as_ref().map(|value| value.0.as_str())
    }
}

async fn ensure_manifest_exists(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<()> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM source_manifests WHERE source_id = ?1 AND generation = ?2",
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?
    .is_some();
    if exists {
        return Ok(());
    }
    Err(ApiError::new(
        "source.ledger.committed_manifest_missing",
        ErrorStage::Diffing,
        format!("committed manifest {} is missing", generation.0),
    )
    .with_source_id(source_id.0.clone()))
}

async fn previous_fingerprints(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<BTreeMap<String, PreviousFingerprint>> {
    let rows = sqlx::query(
        "SELECT source_item_key, content_hash, version, mtime
         FROM source_items
         WHERE source_id = ?1 AND generation = ?2
         ORDER BY source_item_key",
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_all(&store.pool)
    .await
    .map_err(sqlite_error)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("source_item_key"),
                PreviousFingerprint {
                    content_hash: row.get("content_hash"),
                    version: row.get("version"),
                    mtime: row.get("mtime"),
                },
            )
        })
        .collect())
}

pub(super) async fn read_manifest_items(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    item_keys: Vec<SourceItemKey>,
) -> Result<Vec<ManifestItem>> {
    if item_keys.is_empty() {
        return Ok(Vec::new());
    }
    const QUERY_BATCH_SIZE: usize = 300;
    let mut items: Vec<ManifestItem> = Vec::with_capacity(item_keys.len());
    for keys in item_keys.chunks(QUERY_BATCH_SIZE) {
        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT item_json FROM source_items WHERE source_id = ");
        query
            .push_bind(&source_id.0)
            .push(" AND generation = ")
            .push_bind(&generation.0)
            .push(" AND source_item_key IN (");
        let mut separated = query.separated(", ");
        for key in keys {
            separated.push_bind(&key.0);
        }
        separated.push_unseparated(")");
        for row in query
            .build()
            .fetch_all(&store.pool)
            .await
            .map_err(sqlite_error)?
        {
            let item_json: String = row.get("item_json");
            items.push(serde_json::from_str(&item_json).map_err(json_error)?);
        }
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

pub(super) async fn read_manifest_items_with_metadata_key(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    item_keys: Vec<SourceItemKey>,
    metadata_key: &str,
) -> Result<Vec<ManifestItem>> {
    if item_keys.is_empty() {
        return Ok(Vec::new());
    }
    const QUERY_BATCH_SIZE: usize = 300;
    let escaped_key = metadata_key.replace('\"', "\\\"");
    let metadata_path = format!("$.metadata.\"{escaped_key}\"");
    let mut items: Vec<ManifestItem> = Vec::new();
    for keys in item_keys.chunks(QUERY_BATCH_SIZE) {
        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT item_json FROM source_items WHERE source_id = ");
        query
            .push_bind(&source_id.0)
            .push(" AND generation = ")
            .push_bind(&generation.0)
            .push(" AND source_item_key IN (");
        let mut separated = query.separated(", ");
        for key in keys {
            separated.push_bind(&key.0);
        }
        separated.push_unseparated(")");
        query
            .push(" AND json_type(item_json, ")
            .push_bind(&metadata_path)
            .push(") IS NOT NULL");
        for row in query
            .build()
            .fetch_all(&store.pool)
            .await
            .map_err(sqlite_error)?
        {
            let item_json: String = row.get("item_json");
            items.push(serde_json::from_str(&item_json).map_err(json_error)?);
        }
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

pub(super) async fn read_manifest_metadata(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Option<MetadataMap>> {
    let row = sqlx::query(
        "SELECT json_extract(manifest_json, '$.metadata') AS metadata_json \
         FROM source_manifests WHERE source_id = ?1 AND generation = ?2",
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;
    row.map(|row| {
        let metadata_json: String = row.get("metadata_json");
        serde_json::from_str(&metadata_json).map_err(json_error)
    })
    .transpose()
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

    let Some(row) = row else {
        return Ok(None);
    };
    let manifest_json: String = row.get("manifest_json");
    let item_rows = sqlx::query(
        "SELECT item_json FROM source_items WHERE source_id = ?1 AND generation = ?2 \
         ORDER BY source_item_key",
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_all(&store.pool)
    .await
    .map_err(sqlite_error)?;
    let items = item_rows
        .into_iter()
        .map(|row| {
            let item_json: String = row.get("item_json");
            serde_json::from_str(&item_json).map_err(json_error)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(reconstruct_manifest(&manifest_json, items)?))
}
