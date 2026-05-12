use crate::core::config::Config;
use crate::jobs::backend::JobKind;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

/// Open a SQLite pool, enable WAL mode, and run all migrations.
///
/// Pass `":memory:"` for in-memory databases (tests).
pub async fn open_sqlite_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    if path != ":memory:"
        && let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        // Use ensure_private_dir (mode 0o700) so SQLite WAL/SHM files —
        // which inherit umask defaults and may contain credential
        // snapshots from job payloads — are not group/world-readable
        // on multi-user hosts.
        //
        // Failure policy: if the parent is under ~/.axon/, hard-fail —
        // SQLite would otherwise create the dir at default umask
        // and silently expose secrets. For paths the operator chose
        // explicitly (AXON_SQLITE_PATH=/var/lib/axon/...), warn-and-
        // continue so non-secret operator-managed locations still work.
        if let Err(e) = crate::core::paths::ensure_private_dir_async(parent.to_path_buf()).await {
            let parent_under_axon_home =
                crate::core::paths::axon_home_dir().is_some_and(|home| parent.starts_with(&home));
            if parent_under_axon_home {
                return Err(sqlx::Error::Configuration(
                    format!(
                        "lite: refusing to open SQLite at {} because parent dir {} could not be created at 0o700: {e}",
                        path,
                        parent.display()
                    )
                    .into(),
                ));
            }
            tracing::warn!(path = %parent.display(), error = %e, "lite: failed to create SQLite parent dir at 0o700");
        }
    }

    let connect_str = if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{}?mode=rwc", path)
    };

    // Pre-create the file at 0o600 before SQLite connects to eliminate the TOCTOU
    // window where the DB is world-readable (default umask is typically 0644).
    // SQLite opens the existing file rather than creating a new one when the path exists.
    #[cfg(unix)]
    if path != ":memory:" {
        use std::os::unix::fs::OpenOptionsExt;
        if let Err(e) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            tracing::warn!(path = %path, error = %e, "lite: failed to pre-create SQLite file at 0o600; DB may be world-readable until chmod runs");
        }
    }

    let opts: SqliteConnectOptions = connect_str.parse()?;
    let opts = opts
        .pragma("journal_mode", "WAL")
        .pragma("busy_timeout", "10000")
        .pragma("foreign_keys", "ON");

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(opts)
        .await?;

    sqlx::migrate!("src/jobs/lite/migrations")
        .run(&pool)
        .await
        .map_err(|e| sqlx::Error::Configuration(e.into()))?;

    Ok(pool)
}

/// Error text written into `error_text` when the watchdog reclaims a stale running job.
///
/// All three read sites (`jobs/lite/store.rs`, `cli/commands/status.rs`,
/// `jobs/lite/ops/tests.rs`) import this constant so a one-character change never
/// silently breaks the renderer.
pub(crate) const RECLAIMED_ERROR_TEXT: &str = "reclaimed after unexpected shutdown";

/// Per-kind reclaim count returned by `reclaim_stale_running_jobs_detailed`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReclaimCounts {
    pub crawl: u64,
    pub embed: u64,
    pub extract: u64,
    pub ingest: u64,
}

impl ReclaimCounts {
    pub fn total(&self) -> u64 {
        self.crawl + self.embed + self.extract + self.ingest
    }
}

/// Reclaim jobs stuck in `running` state from a previous crashed process.
/// Returns the total count for backwards compatibility; the watchdog uses
/// `reclaim_stale_running_jobs_detailed` to drive per-kind worker wakeups.
pub async fn reclaim_stale_running_jobs(
    pool: &SqlitePool,
    stale_threshold_ms: i64,
) -> Result<u64, sqlx::Error> {
    Ok(
        reclaim_stale_running_jobs_detailed(pool, stale_threshold_ms)
            .await?
            .total(),
    )
}

pub async fn reclaim_stale_running_jobs_detailed(
    pool: &SqlitePool,
    stale_threshold_ms: i64,
) -> Result<ReclaimCounts, sqlx::Error> {
    let mut counts = ReclaimCounts::default();
    for kind in JobKind::all() {
        let n = reclaim_stale_running_jobs_for_table(pool, *kind, stale_threshold_ms)
            .await
            .inspect_err(|e| {
                tracing::error!(table = kind.table_name(), error = %e, "watchdog: per-table sweep failed");
            })?;
        match kind {
            JobKind::Crawl => counts.crawl = n,
            JobKind::Embed => counts.embed = n,
            JobKind::Extract => counts.extract = n,
            JobKind::Ingest => counts.ingest = n,
        }
    }
    let total = counts.total();
    if total > 0 {
        tracing::info!(reclaimed = total, "watchdog: sweep complete");
    }
    Ok(counts)
}

pub async fn reclaim_stale_running_jobs_for_table(
    pool: &SqlitePool,
    kind: JobKind,
    stale_threshold_ms: i64,
) -> Result<u64, sqlx::Error> {
    // SAFETY: `kind.table_name()` returns a compile-time `&'static str` from
    // a closed enum dispatch; no caller-controlled value reaches `format!`.
    // Status literals come from a closed enum too.
    let table = kind.table_name();
    let threshold = now_ms() - stale_threshold_ms;
    let stale_ids = sqlx::query_scalar::<_, String>(&format!(
        "SELECT id FROM {} WHERE status='running' AND updated_at < ?",
        table
    ))
    .bind(threshold)
    .fetch_all(pool)
    .await?;
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='pending', error_text=?, \
         updated_at=? WHERE status='running' AND updated_at < ?",
        table
    ))
    .bind(RECLAIMED_ERROR_TEXT)
    .bind(now_ms())
    .bind(threshold)
    .execute(pool)
    .await?;
    let n = result.rows_affected();
    if n > 0 {
        for job_id in stale_ids.iter().take(n as usize) {
            tracing::warn!(
                table,
                job_id = %job_id,
                "watchdog: reclaimed stale running job and reset it to pending"
            );
        }
        tracing::info!(
            table,
            reclaimed = n,
            "watchdog: reclaimed stale running jobs"
        );
    }
    Ok(n)
}

/// Reclaim stale watch leases from a previous crashed process.
///
/// Clears `lease_expires_at` for any `axon_watch_defs` row whose lease has
/// already expired so the scheduler can re-acquire them immediately.
pub async fn reclaim_stale_watch_leases(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let now = now_ms();
    let result = sqlx::query(
        "UPDATE axon_watch_defs SET lease_expires_at = NULL WHERE lease_expires_at < ?",
    )
    .bind(now)
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn reclaim_stale_running_jobs_only_reclaims_stale_running_rows() {
        let pool = open_sqlite_pool(":memory:").await.expect("pool");
        let stale_id = Uuid::new_v4().to_string();
        let fresh_id = Uuid::new_v4().to_string();
        let pending_id = Uuid::new_v4().to_string();
        let stale_updated_at = now_ms() - 10_000;
        let fresh_updated_at = now_ms();

        for (id, status, updated_at) in [
            (&stale_id, "running", stale_updated_at),
            (&fresh_id, "running", fresh_updated_at),
            (&pending_id, "pending", stale_updated_at),
        ] {
            sqlx::query(
                "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
                 VALUES (?, ?, ?, '{}', ?, ?)",
            )
            .bind(id)
            .bind(status)
            .bind("test input")
            .bind(updated_at)
            .bind(updated_at)
            .execute(&pool)
            .await
            .expect("insert job");
        }

        let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Embed, 5_000)
            .await
            .expect("reclaim");

        assert_eq!(reclaimed, 1);
        let stale_status: String =
            sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
                .bind(&stale_id)
                .fetch_one(&pool)
                .await
                .expect("stale status");
        let fresh_status: String =
            sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
                .bind(&fresh_id)
                .fetch_one(&pool)
                .await
                .expect("fresh status");
        let pending_status: String =
            sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
                .bind(&pending_id)
                .fetch_one(&pool)
                .await
                .expect("pending status");

        assert_eq!(stale_status, "pending");
        assert_eq!(fresh_status, "running");
        assert_eq!(pending_status, "pending");
    }

    #[tokio::test]
    async fn reclaim_stale_running_jobs_for_table_sets_reclaim_error_text() {
        let pool = open_sqlite_pool(":memory:").await.expect("pool");
        let stale_updated_at = now_ms() - 10_000;

        let stale_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO axon_crawl_jobs \
             (id, status, url, config_json, created_at, updated_at, started_at) \
             VALUES (?, 'running', 'https://stale.example', '{}', ?, ?, ?)",
        )
        .bind(&stale_id)
        .bind(stale_updated_at)
        .bind(stale_updated_at)
        .bind(stale_updated_at)
        .execute(&pool)
        .await
        .unwrap();

        let fresh_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO axon_crawl_jobs \
             (id, status, url, config_json, created_at, updated_at) \
             VALUES (?, 'running', 'https://fresh.example', '{}', ?, ?)",
        )
        .bind(&fresh_id)
        .bind(now_ms())
        .bind(now_ms())
        .execute(&pool)
        .await
        .unwrap();

        let pending_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO axon_crawl_jobs \
             (id, status, url, config_json, created_at, updated_at) \
             VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
        )
        .bind(&pending_id)
        .bind(stale_updated_at)
        .bind(stale_updated_at)
        .execute(&pool)
        .await
        .unwrap();

        let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000)
            .await
            .expect("reclaim");

        assert_eq!(
            reclaimed, 1,
            "only the stale running row should be reclaimed"
        );

        let (status, error_text): (String, Option<String>) =
            sqlx::query_as("SELECT status, error_text FROM axon_crawl_jobs WHERE id = ?")
                .bind(&stale_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(error_text.as_deref(), Some(RECLAIMED_ERROR_TEXT));

        let fresh_status: String =
            sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
                .bind(&fresh_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fresh_status, "running", "fresh row must not be reclaimed");

        let pending_status: String =
            sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
                .bind(&pending_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending_status, "pending", "pending row must not be touched");
    }
}
