use sqlx::{SqliteConnection, SqlitePool, pool::PoolConnection};
use uuid::Uuid;

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload, JobSidecarPayload};
use crate::jobs::error::JobError;
use crate::jobs::store::now_ms;

/// Reject the enqueue when the queue is at or above its pending cap.
///
/// `cap == 0` is treated as unlimited. Generic over the executor so callers
/// can pass `&SqlitePool` (test helpers) or `&mut SqliteConnection` (the
/// immediate-transaction path used by `enqueue_job`).
///
/// Implementation note: we ask SQLite to return at most `cap + 1` row
/// stubs (`SELECT 1 ... LIMIT cap+1`) instead of `SELECT COUNT(*)`. Cost is
/// O(cap) on the `(status, created_at DESC)` index added in migration 0004,
/// not O(pending) — caps below ~100 stay constant-time even when the
/// pending queue grows large.
///
/// SAFETY: `kind.table_name()` returns a compile-time `&'static str` from a
/// closed enum dispatch; no caller-controlled value reaches `format!`. Do
/// not change to accept a runtime-derived table name.
pub(super) async fn check_pending_cap_for<'e, E>(
    executor: E,
    kind: JobKind,
    cap: u64,
) -> Result<(), JobError>
where
    E: sqlx::SqliteExecutor<'e>,
{
    if cap == 0 {
        return Ok(());
    }
    let table = kind.table_name();
    let limit = i64::try_from(cap.saturating_add(1)).unwrap_or(i64::MAX);
    let query = format!("SELECT 1 FROM {table} WHERE status = 'pending' LIMIT ?");
    let rows: Vec<i64> = sqlx::query_scalar(&query)
        .bind(limit)
        .fetch_all(executor)
        .await?;
    let observed = rows.len() as u64;
    if observed >= cap {
        return Err(JobError::QueueCapacityExceeded {
            kind: kind.queue_name(),
            cap,
            current: observed,
        });
    }
    Ok(())
}

/// Acquire a dedicated connection and open a `BEGIN IMMEDIATE` transaction on
/// it. sqlx's `pool.begin()` always emits plain `BEGIN` (deferred), and a
/// follow-up `BEGIN IMMEDIATE` on the same connection errors as a nested
/// transaction — so we go through a raw connection instead. The caller is
/// responsible for issuing `COMMIT` (or `ROLLBACK`) before dropping the
/// connection.
async fn begin_immediate(pool: &SqlitePool) -> Result<PoolConnection<sqlx::Sqlite>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    Ok(conn)
}

async fn commit(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    Ok(())
}

async fn rollback_best_effort(conn: &mut SqliteConnection) {
    if let Err(e) = sqlx::query("ROLLBACK").execute(&mut *conn).await {
        tracing::warn!(error = %e, "enqueue: ROLLBACK after failed transaction errored");
    }
}

/// Insert a new job row with status='pending'. Returns the new job's UUID.
///
/// Pending-queue caps are sourced from `cfg.max_pending_{crawl,embed,extract,ingest}_jobs`
/// (priority CLI flag > env > TOML > default). Pass `&Config::default_minimal()` from
/// tests to use the built-in defaults (100/50/50/50) — those are well above any
/// reasonable test fixture so production caps don't accidentally fail tests.
///
/// The cap check and the INSERT run inside a `BEGIN IMMEDIATE` transaction so
/// concurrent enqueues serialize on the SQLite RESERVED write lock — without
/// it, two callers can both observe `count=0`, pass the cap check, and
/// double-insert past the configured cap.
pub async fn enqueue_job(
    pool: &SqlitePool,
    payload: &JobPayload,
    cfg: &Config,
) -> Result<Uuid, JobError> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let id_str = id.to_string();
    let kind = payload.kind();
    let cap = cap_for(kind, cfg);

    let mut conn = begin_immediate(pool).await?;

    let result: Result<(), JobError> = async {
        check_pending_cap_for(&mut *conn, kind, cap).await?;
        insert_payload(&mut conn, &id_str, now, payload).await
    }
    .await;

    match result {
        Ok(()) => match commit(&mut conn).await {
            Ok(()) => Ok(id),
            Err(commit_err) => {
                // sqlx doesn't auto-rollback on PoolConnection::drop, so a
                // failed COMMIT would leave the next pool checkout inside a
                // stale BEGIN IMMEDIATE.
                rollback_best_effort(&mut conn).await;
                Err(commit_err.into())
            }
        },
        Err(e) => {
            rollback_best_effort(&mut conn).await;
            Err(e)
        }
    }
}

pub async fn enqueue_job_with_sidecar(
    pool: &SqlitePool,
    payload: &JobPayload,
    sidecar: &JobSidecarPayload,
    cfg: &Config,
) -> Result<Uuid, JobError> {
    let kind = payload.kind();
    if sidecar.kind() != kind {
        return Err(JobError::Other(format!(
            "sidecar kind {:?} does not match job kind {:?}",
            sidecar.kind(),
            kind
        )));
    }

    let id = Uuid::new_v4();
    let now = now_ms();
    let id_str = id.to_string();
    let cap = cap_for(kind, cfg);

    let mut conn = begin_immediate(pool).await?;

    let result: Result<(), JobError> = async {
        check_pending_cap_for(&mut *conn, kind, cap).await?;
        insert_payload(&mut conn, &id_str, now, payload).await?;
        insert_sidecar_payload(&mut conn, &id_str, now, sidecar).await
    }
    .await;

    match result {
        Ok(()) => match commit(&mut conn).await {
            Ok(()) => Ok(id),
            Err(commit_err) => {
                rollback_best_effort(&mut conn).await;
                Err(commit_err.into())
            }
        },
        Err(e) => {
            rollback_best_effort(&mut conn).await;
            Err(e)
        }
    }
}

fn cap_for(kind: JobKind, cfg: &Config) -> u64 {
    match kind {
        JobKind::Crawl => cfg.max_pending_crawl_jobs as u64,
        JobKind::Embed => cfg.max_pending_embed_jobs as u64,
        JobKind::Extract => cfg.max_pending_extract_jobs as u64,
        JobKind::Ingest => cfg.max_pending_ingest_jobs as u64,
    }
}

async fn insert_payload(
    conn: &mut SqliteConnection,
    id_str: &str,
    now: i64,
    payload: &JobPayload,
) -> Result<(), JobError> {
    match payload {
        JobPayload::Crawl { url, config_json } => {
            sqlx::query(
                "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(id_str)
            .bind(url)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        }
        JobPayload::Embed { input, config_json } => {
            // Stamp fs_namespace for local-path inputs so workers in a different
            // filesystem namespace (e.g. the host CLI) do not claim container-path
            // jobs they cannot read. URL/free-text inputs get NULL — claimable by
            // any worker. Mirrors `select::looks_path_like` without importing
            // `src/vector` from `src/jobs`.
            let ns = if embed_input_looks_like_local_path(input) {
                std::env::var("AXON_FS_NAMESPACE").ok()
            } else {
                None
            };
            sqlx::query(
                "INSERT INTO axon_embed_jobs \
                 (id, status, input_text, config_json, created_at, updated_at, fs_namespace) \
                 VALUES (?, 'pending', ?, ?, ?, ?, ?)",
            )
            .bind(id_str)
            .bind(input)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .bind(ns)
            .execute(&mut *conn)
            .await?;
        }
        JobPayload::Extract { urls, config_json } => {
            let urls_json =
                serde_json::to_string(urls).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO axon_extract_jobs (id, status, urls_json, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(id_str)
            .bind(&urls_json)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        }
        JobPayload::Ingest {
            target,
            source_type,
            config_json,
        } => {
            sqlx::query(
                "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?, ?)",
            )
            .bind(id_str)
            .bind(target)
            .bind(source_type)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// True when an embed input string looks like a filesystem path rather than a
/// URL or free-text query. Used to decide whether to stamp `fs_namespace` at
/// enqueue time. Mirrors `vector::ops::input::select::looks_path_like` to
/// avoid a `src/jobs → src/vector` dependency.
fn embed_input_looks_like_local_path(input: &str) -> bool {
    let input = input.trim();
    if input.is_empty() {
        return false;
    }
    let bytes = input.as_bytes();
    let windows_drive = input.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    input.starts_with('/')
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with("~/")
        || input.starts_with("\\\\")
        || windows_drive
}

async fn insert_sidecar_payload(
    conn: &mut SqliteConnection,
    id_str: &str,
    now: i64,
    sidecar: &JobSidecarPayload,
) -> Result<(), JobError> {
    match sidecar {
        JobSidecarPayload::IngestPreparedSessions { payload_json } => {
            sqlx::query(
                "INSERT INTO axon_ingest_payloads (job_id, payload_kind, payload_json, created_at) \
                 VALUES (?, 'prepared_sessions', ?, ?)",
            )
            .bind(id_str)
            .bind(payload_json)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}
