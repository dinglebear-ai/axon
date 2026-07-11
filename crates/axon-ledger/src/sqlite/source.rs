use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::json_error;
use crate::store::Result;

pub(super) async fn upsert_source(store: &SqliteLedgerStore, source: SourceSummary) -> Result<()> {
    let source_id = source.source_id.0.clone();
    let summary_json = serde_json::to_string(&source).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO sources (
            source_id,
            summary_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(source_id) DO UPDATE SET
            summary_json = excluded.summary_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(source_id)
    .bind(summary_json)
    .bind(source.created_at.0)
    .bind(source.updated_at.0)
    .execute(&store.pool)
    .await
    .map_err(sqlite_error)?;

    Ok(())
}

pub(super) async fn get_source(
    store: &SqliteLedgerStore,
    source_id: SourceId,
) -> Result<Option<SourceSummary>> {
    let row = sqlx::query(
        r#"
        SELECT summary_json
        FROM sources
        WHERE source_id = ?1
        "#,
    )
    .bind(source_id.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;

    row.map(|row| {
        let summary_json: String = row.get("summary_json");
        serde_json::from_str(&summary_json).map_err(json_error)
    })
    .transpose()
}

/// List all registered sources, then filter/paginate via [`crate::listing`] so
/// this stays in lockstep with `FakeLedgerStore::list_sources`.
///
/// Sources are stored as an opaque `summary_json` blob keyed by `source_id`
/// (see `upsert_source`), so filtering happens in Rust rather than via
/// `json_extract` `WHERE` clauses — simpler, and correct for the tag/query
/// substring filters that don't map cleanly onto SQL. The empty-store
/// assumption in `docs/pipeline-unification/runtime/ledger-contract.md` means
/// this full scan is bounded by the same source-count expectations as the rest
/// of the ledger.
pub(super) async fn list_sources(
    store: &SqliteLedgerStore,
    request: SourceListRequest,
) -> Result<Page<SourceSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT summary_json
        FROM sources
        ORDER BY source_id ASC
        "#,
    )
    .fetch_all(&store.pool)
    .await
    .map_err(sqlite_error)?;

    let sources = rows
        .into_iter()
        .map(|row| {
            let summary_json: String = row.get("summary_json");
            serde_json::from_str::<SourceSummary>(&summary_json).map_err(json_error)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(crate::listing::list_page(sources, &request))
}

pub(super) async fn foreign_keys_enabled(store: &SqliteLedgerStore) -> Result<bool> {
    let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&store.pool)
        .await
        .map_err(sqlite_error)?;
    Ok(enabled == 1)
}
