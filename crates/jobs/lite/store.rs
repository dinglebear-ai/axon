use crate::crates::core::config::Config;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// Open a SQLite pool, enable WAL mode, and run all migrations.
///
/// Pass `":memory:"` for in-memory databases (tests).
pub async fn open_sqlite_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    if path != ":memory:"
        && let Some(parent) = std::path::Path::new(path).parent()
    {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    let connect_str = if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{}?mode=rwc", path)
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&connect_str)
        .await?;

    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;

    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool)
        .await?;

    sqlx::migrate!("crates/jobs/lite/migrations")
        .run(&pool)
        .await
        .map_err(|e| sqlx::Error::Configuration(e.into()))?;

    Ok(pool)
}

/// Reclaim jobs stuck in `running` state from a previous crashed process.
pub async fn reclaim_stale_running_jobs(
    pool: &SqlitePool,
    stale_threshold_ms: i64,
) -> Result<u64, sqlx::Error> {
    let threshold = now_ms() - stale_threshold_ms;
    let mut total: u64 = 0;

    for table in &[
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
        "axon_refresh_jobs",
        "axon_graph_jobs",
    ] {
        let result = sqlx::query(&format!(
            "UPDATE {} SET status='pending', error_text='reclaimed after unexpected shutdown', \
             updated_at=? WHERE status='running' AND updated_at < ?",
            table
        ))
        .bind(now_ms())
        .bind(threshold)
        .execute(pool)
        .await?;

        total += result.rows_affected();
    }

    Ok(total)
}

pub async fn reclaim_stale_running_jobs_for_table(
    pool: &SqlitePool,
    table: &str,
    stale_threshold_ms: i64,
) -> Result<u64, sqlx::Error> {
    let threshold = now_ms() - stale_threshold_ms;
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='pending', error_text='reclaimed after unexpected shutdown', \
         updated_at=? WHERE status='running' AND updated_at < ?",
        table
    ))
    .bind(now_ms())
    .bind(threshold)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Open a SQLite pool from the path stored in `cfg.sqlite_path`.
///
/// Shared by `services/jobs.rs` and `jobs/watch_lite.rs` to avoid duplicating
/// the `open_sqlite_pool(&cfg.sqlite_path.to_string_lossy())` call pattern.
pub(crate) async fn open_config_pool(cfg: &Config) -> Result<SqlitePool, sqlx::Error> {
    open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await
}

/// Current time as Unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
