use std::sync::LazyLock;

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::jobs::backend::JobPayload;
use crate::jobs::error::JobError;
use crate::jobs::lite::store::now_ms;

/// Parse a queue cap env var once at process start. Returns `None` if unset
/// or unparseable; `Some(0)` means explicitly unlimited; `Some(n)` is the cap.
///
/// On a parse failure, logs a `tracing::warn!` so misconfiguration is visible
/// rather than silently disabling the cap.
fn parse_cap_env(name: &'static str) -> Option<u64> {
    match std::env::var(name) {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(
                    env_var = name,
                    raw = %raw,
                    error = %e,
                    "queue cap env var is set but not a valid u64; treating as unset"
                );
                None
            }
        },
        Err(_) => None,
    }
}

/// `AXON_MAX_PENDING_CRAWL_JOBS` (default 100, `0` = unlimited).
static CRAWL_CAP: LazyLock<u64> =
    LazyLock::new(|| parse_cap_env("AXON_MAX_PENDING_CRAWL_JOBS").unwrap_or(100));

/// `AXON_MAX_PENDING_EMBED_JOBS` (default 50, `0` = unlimited).
static EMBED_CAP: LazyLock<u64> =
    LazyLock::new(|| parse_cap_env("AXON_MAX_PENDING_EMBED_JOBS").unwrap_or(50));

/// `AXON_MAX_PENDING_EXTRACT_JOBS` (default 50, `0` = unlimited).
static EXTRACT_CAP: LazyLock<u64> =
    LazyLock::new(|| parse_cap_env("AXON_MAX_PENDING_EXTRACT_JOBS").unwrap_or(50));

/// `AXON_MAX_PENDING_INGEST_JOBS` (default 50, `0` = unlimited).
static INGEST_CAP: LazyLock<u64> =
    LazyLock::new(|| parse_cap_env("AXON_MAX_PENDING_INGEST_JOBS").unwrap_or(50));

/// Check whether the pending job count for a given queue is at or above the configured cap.
///
/// `table` is the SQL table name (e.g. `axon_crawl_jobs`).
/// `queue_name` is the human-readable queue name used in the error variant.
/// `cap` is the cap (`0` = unlimited; `>= 1` rejects when `pending >= cap`).
///
/// Returns `Err(JobError::QueueCapacityExceeded { .. })` when the queue is full.
///
/// # SQL injection safety
///
/// `table` is interpolated directly into the SQL string because sqlx does not
/// support binding a table name. The invariant is that `table` is a compile-time
/// `&'static str` literal, supplied only by the `enqueue_job` callsites in this
/// file (`"axon_crawl_jobs"`, `"axon_embed_jobs"`, `"axon_extract_jobs"`,
/// `"axon_ingest_jobs"`). Do **not** call this function with attacker- or
/// caller-controlled `table` values.
pub(super) async fn check_pending_cap_for(
    pool: &SqlitePool,
    table: &'static str,
    queue_name: &'static str,
    cap: u64,
) -> Result<(), JobError> {
    if cap == 0 {
        return Ok(());
    }
    let query = format!("SELECT COUNT(*) FROM {table} WHERE status = 'pending'");
    let count_i64: i64 = sqlx::query_scalar(&query).fetch_one(pool).await?;
    // SQLite COUNT(*) is non-negative in practice, but defend against a wrapping
    // cast: clamp negatives to 0 and refuse to consult the cap if conversion fails.
    let count: u64 = u64::try_from(count_i64).unwrap_or(0);
    if count >= cap {
        return Err(JobError::QueueCapacityExceeded {
            kind: queue_name,
            cap,
            current: count,
        });
    }
    Ok(())
}

/// Insert a new job row with status='pending'. Returns the new job's UUID.
pub async fn enqueue_job(pool: &SqlitePool, payload: &JobPayload) -> Result<Uuid, JobError> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let id_str = id.to_string();

    match payload {
        JobPayload::Crawl { url, config_json } => {
            check_pending_cap_for(pool, "axon_crawl_jobs", "crawl", *CRAWL_CAP).await?;
            sqlx::query(
                "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(url)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Embed { input, config_json } => {
            check_pending_cap_for(pool, "axon_embed_jobs", "embed", *EMBED_CAP).await?;
            sqlx::query(
                "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(input)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Extract { urls, config_json } => {
            check_pending_cap_for(pool, "axon_extract_jobs", "extract", *EXTRACT_CAP).await?;
            let urls_json =
                serde_json::to_string(urls).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO axon_extract_jobs (id, status, urls_json, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(&urls_json)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Ingest {
            target,
            source_type,
            config_json,
        } => {
            check_pending_cap_for(pool, "axon_ingest_jobs", "ingest", *INGEST_CAP).await?;
            sqlx::query(
                "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(target)
            .bind(source_type)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(id)
}
