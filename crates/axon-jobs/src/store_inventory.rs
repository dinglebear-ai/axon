use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyJobTable {
    pub table: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyJobStoreBlocker {
    pub legacy_tables: Vec<String>,
    pub tables: Vec<LegacyJobTable>,
    pub message: String,
}

pub async fn detect_incompatible_legacy_jobs(
    pool: &SqlitePool,
) -> Result<Option<LegacyJobStoreBlocker>, ApiError> {
    if has_cutover_receipt(pool).await? {
        return Ok(None);
    }

    let mut tables = Vec::new();
    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        let row_count = legacy_row_count(pool, table).await?;
        if row_count > 0 {
            tables.push(LegacyJobTable {
                table: table.to_string(),
                row_count,
            });
        }
    }

    if tables.is_empty() {
        return Ok(None);
    }

    let legacy_tables = tables
        .iter()
        .map(|table| table.table.clone())
        .collect::<Vec<_>>();
    let counts = tables
        .iter()
        .map(|table| format!("{}={} rows", table.table, table.row_count))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(LegacyJobStoreBlocker {
        legacy_tables,
        tables,
        message: format!(
            "incompatible legacy job rows detected ({counts}); run axon reset or record a preflight clean-cutover receipt before starting unified workers"
        ),
    }))
}

async fn has_cutover_receipt(pool: &SqlitePool) -> Result<bool, ApiError> {
    if !table_exists(pool, "axon_job_cutover_receipts").await? {
        return Ok(false);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_job_cutover_receipts
         WHERE receipt_kind IN ('legacy_reset', 'preflight_clean_cutover')",
    )
    .fetch_one(pool)
    .await
    .map_err(sql_error)?;
    Ok(count > 0)
}

async fn legacy_row_count(pool: &SqlitePool, table: &str) -> Result<u64, ApiError> {
    if !table_exists(pool, table).await? {
        return Ok(0);
    }
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    let count: i64 = sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .map_err(sql_error)?
        .get("count");
    Ok(count as u64)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, ApiError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(sql_error)?;
    Ok(count > 0)
}

fn sql_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "job_store.inventory_sql",
        ErrorStage::Planning,
        error.to_string(),
    )
}
