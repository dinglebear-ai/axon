//! Indexed bulk reads and conditional generation carry of document statuses.
use super::*;
use std::collections::BTreeSet;

fn conflict() -> ApiError {
    ApiError::new(
        "source.ledger.status_provenance_changed",
        ErrorStage::Publishing,
        "document status provenance changed; refresh must reprepare retained items",
    )
}

pub(in crate::sqlite) async fn document_statuses_for_items(
    store: &SqliteLedgerStore,
    source_id: SourceId,
    item_keys: Vec<SourceItemKey>,
) -> Result<Vec<DocumentStatus>> {
    let mut connection = store.pool.acquire().await.map_err(sqlite_error)?;
    read_statuses(&mut connection, &source_id, &item_keys).await
}

async fn read_statuses(
    connection: &mut sqlx::SqliteConnection,
    source: &SourceId,
    keys: &[SourceItemKey],
) -> Result<Vec<DocumentStatus>> {
    let keys = keys
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for keys in keys.chunks(400) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT status_json FROM document_status WHERE source_id = ",
        );
        query.push_bind(&source.0).push(" AND source_item_key IN (");
        let mut list = query.separated(",");
        for key in keys {
            list.push_bind(&key.0);
        }
        list.push_unseparated(")");
        #[cfg(test)]
        crate::sqlite_tests::bulk_lookup_tests::record_query(source);
        for row in query
            .build()
            .fetch_all(&mut *connection)
            .await
            .map_err(sqlite_error)?
        {
            output.push(
                serde_json::from_str::<DocumentStatus>(&row.get::<String, _>("status_json"))
                    .map_err(json_error)?,
            );
        }
    }
    output.sort_by(|a, b| a.document_id.cmp(&b.document_id));
    Ok(output)
}

pub(in crate::sqlite) async fn carry_document_statuses(
    store: &SqliteLedgerStore,
    source_id: SourceId,
    expected_generation: SourceGenerationId,
    next_generation: SourceGenerationId,
    mut expected_statuses: Vec<DocumentStatus>,
    updated_at: Timestamp,
) -> Result<u64> {
    if expected_statuses.is_empty() {
        return Ok(0);
    }
    let mut tx = ImmediateTx::begin_with_gate(&store.pool, &store.write_gate)
        .await
        .map_err(sqlite_error)?;
    let committed: Option<String> =
        sqlx::query_scalar("SELECT committed_generation FROM sources WHERE source_id = ?")
            .bind(&source_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(sqlite_error)?
            .flatten();
    if committed.as_deref() != Some(expected_generation.0.as_str()) {
        return Err(conflict());
    }
    let target_json: Option<String> = sqlx::query_scalar(
        "SELECT generation_json FROM source_generations WHERE source_id = ? AND generation = ?",
    )
    .bind(&source_id.0)
    .bind(&next_generation.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    let target: SourceGeneration =
        serde_json::from_str(&target_json.ok_or_else(conflict)?).map_err(json_error)?;
    if target.previous_generation.as_ref() != Some(&expected_generation)
        || next_generation == expected_generation
    {
        return Err(conflict());
    }
    expected_statuses.sort_by(|a, b| a.document_id.cmp(&b.document_id));
    if expected_statuses.iter().any(|s| {
        !matches!(
            s.status,
            DocumentLifecycleStatus::Prepared
                | DocumentLifecycleStatus::Embedded
                | DocumentLifecycleStatus::Vectorized
                | DocumentLifecycleStatus::Published
                | DocumentLifecycleStatus::Skipped
        ) || s.source_id != source_id
            || s.generation.as_ref() != Some(&expected_generation)
            || s.updated_at.0 > updated_at.0
    }) || expected_statuses
        .windows(2)
        .any(|w| w[0].document_id == w[1].document_id)
    {
        return Err(conflict());
    }
    let keys = expected_statuses
        .iter()
        .map(|s| s.source_item_key.clone())
        .collect::<Vec<_>>();
    if read_statuses(&mut tx, &source_id, &keys).await? != expected_statuses {
        return Err(conflict());
    }
    for status in &mut expected_statuses {
        status.generation = Some(next_generation.clone());
        status.updated_at = updated_at.clone();
    }
    for statuses in expected_statuses.chunks(DOCUMENT_STATUS_TX_BATCH_SIZE) {
        let writes = status_writes(statuses)?;
        validate_status_items(&mut tx, &writes).await?;
        upsert_status_batch(&mut tx, &writes).await?;
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(expected_statuses.len() as u64)
}
