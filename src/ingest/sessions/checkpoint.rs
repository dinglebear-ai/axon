use crate::ingest::sessions::watch::validate::ValidatedSessionPath;
use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileMetadata {
    pub canonical: PathBuf,
    pub path_hash: String,
    pub provider: String,
    pub basename: String,
    pub redacted_display: String,
    pub file_size: u64,
    pub file_mtime_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SessionWatchError {
    pub path_hash: String,
    pub provider: String,
    pub basename: String,
    pub error_code: String,
    pub error_redacted: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SessionWatchStatus {
    pub checkpoint_count: i64,
    pub error_count: i64,
    pub recent_errors: Vec<SessionWatchError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWatchCheckpointRecord {
    pub file_size: u64,
    pub file_mtime_ms: i64,
    pub content_hash: Option<String>,
    pub last_error_code: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionWatchCheckpointState {
    LocalIngested,
    NoContent,
    RemoteAccepted,
    Error,
}

impl SessionWatchCheckpointState {
    fn as_str(self) -> &'static str {
        match self {
            Self::LocalIngested => "local_ingested",
            Self::NoContent => "no_content",
            Self::RemoteAccepted => "remote_accepted",
            Self::Error => "error",
        }
    }
}

impl SessionFileMetadata {
    pub fn from_validated_path(path: &ValidatedSessionPath) -> Result<Self> {
        let metadata = std::fs::metadata(&path.canonical)?;
        let file_size = metadata.len();
        let file_mtime_ms = metadata
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        Ok(Self {
            canonical: path.canonical.clone(),
            path_hash: path.path_hash.clone(),
            provider: path.provider.as_str().to_string(),
            basename: path.basename.clone(),
            redacted_display: path.redacted_display.clone(),
            file_size,
            file_mtime_ms,
        })
    }
}

pub async fn checkpoint_metadata_matches(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
) -> Result<bool> {
    let row = checkpoint_record_for_path_hash(pool, &meta.path_hash).await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let current_hash = if row.content_hash.is_some() {
        Some(stream_content_hash(&meta.canonical).await?)
    } else {
        None
    };
    let matched = checkpoint_record_matches(meta, &row, current_hash.as_deref());
    if matched && !checkpoint_record_metadata_matches(meta, &row) {
        refresh_checkpoint_metadata(pool, meta).await?;
    }
    Ok(matched)
}

pub async fn checkpoint_records_by_path_hash(
    pool: &SqlitePool,
    path_hashes: &[String],
) -> Result<HashMap<String, SessionWatchCheckpointRecord>> {
    if path_hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", path_hashes.len())
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        r#"
        SELECT path_hash, file_size, file_mtime_ms, content_hash, last_error_code, state
        FROM axon_session_watch_checkpoints
        WHERE path_hash IN ({placeholders})
        "#
    );
    let mut sql = sqlx::query(&query);
    for path_hash in path_hashes {
        sql = sql.bind(path_hash);
    }
    let rows = sql.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let path_hash: String = row.get("path_hash");
            (path_hash, checkpoint_record_from_row(&row))
        })
        .collect())
}

pub async fn checkpoint_record_for_path_hash(
    pool: &SqlitePool,
    path_hash: &str,
) -> Result<Option<SessionWatchCheckpointRecord>> {
    let row = sqlx::query(
        r#"
        SELECT file_size, file_mtime_ms, content_hash, last_error_code, state
        FROM axon_session_watch_checkpoints
        WHERE path_hash = ?
        "#,
    )
    .bind(path_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| checkpoint_record_from_row(&row)))
}

pub fn checkpoint_record_matches(
    meta: &SessionFileMetadata,
    record: &SessionWatchCheckpointRecord,
    current_content_hash: Option<&str>,
) -> bool {
    if record.last_error_code.is_some() || !reusable_checkpoint_state(&record.state) {
        return false;
    }
    if let Some(stored_hash) = record.content_hash.as_deref() {
        return current_content_hash == Some(stored_hash);
    }
    checkpoint_record_metadata_matches(meta, record)
}

pub fn checkpoint_record_metadata_matches(
    meta: &SessionFileMetadata,
    record: &SessionWatchCheckpointRecord,
) -> bool {
    record.file_size == meta.file_size && record.file_mtime_ms == meta.file_mtime_ms
}

pub async fn refresh_checkpoint_metadata(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE axon_session_watch_checkpoints
        SET provider = ?,
            basename = ?,
            redacted_display = ?,
            file_size = ?,
            file_mtime_ms = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE path_hash = ?
        "#,
    )
    .bind(&meta.provider)
    .bind(&meta.basename)
    .bind(&meta.redacted_display)
    .bind(meta.file_size as i64)
    .bind(meta.file_mtime_ms)
    .bind(&meta.path_hash)
    .execute(pool)
    .await?;
    Ok(())
}

fn checkpoint_record_from_row(row: &SqliteRow) -> SessionWatchCheckpointRecord {
    let file_size: i64 = row.get("file_size");
    SessionWatchCheckpointRecord {
        file_size: file_size.max(0) as u64,
        file_mtime_ms: row.get("file_mtime_ms"),
        content_hash: row.get("content_hash"),
        last_error_code: row.get("last_error_code"),
        state: row.get("state"),
    }
}

fn reusable_checkpoint_state(state: &str) -> bool {
    matches!(state, "local_ingested" | "no_content")
}

pub async fn stream_content_hash(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn record_success(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
    content_hash: Option<&str>,
) -> Result<()> {
    record_success_with_state(
        pool,
        meta,
        content_hash,
        SessionWatchCheckpointState::LocalIngested,
    )
    .await
}

pub async fn record_no_content(pool: &SqlitePool, meta: &SessionFileMetadata) -> Result<()> {
    let hash = stream_content_hash(&meta.canonical).await.ok();
    record_success_with_state(
        pool,
        meta,
        hash.as_deref(),
        SessionWatchCheckpointState::NoContent,
    )
    .await
}

async fn record_success_with_state(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
    content_hash: Option<&str>,
    state: SessionWatchCheckpointState,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO axon_session_watch_checkpoints
            (path_hash, provider, basename, redacted_display, file_size, file_mtime_ms, content_hash, state, remote_job_id, failure_count, next_attempt_at, last_indexed_at, last_error_code, last_error_redacted, updated_at)
        VALUES
            (?, ?, ?, ?, ?, ?, ?, ?, NULL, 0, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(path_hash) DO UPDATE SET
            provider = excluded.provider,
            basename = excluded.basename,
            redacted_display = excluded.redacted_display,
            file_size = excluded.file_size,
            file_mtime_ms = excluded.file_mtime_ms,
            content_hash = excluded.content_hash,
            state = excluded.state,
            remote_job_id = NULL,
            failure_count = 0,
            next_attempt_at = NULL,
            last_indexed_at = excluded.last_indexed_at,
            last_error_code = NULL,
            last_error_redacted = NULL,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&meta.path_hash)
    .bind(&meta.provider)
    .bind(&meta.basename)
    .bind(&meta.redacted_display)
    .bind(meta.file_size as i64)
    .bind(meta.file_mtime_ms)
    .bind(content_hash)
    .bind(state.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_remote_accepted(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
    content_hash: Option<&str>,
    remote_job_id: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO axon_session_watch_checkpoints
            (path_hash, provider, basename, redacted_display, file_size, file_mtime_ms, content_hash, state, remote_job_id, failure_count, next_attempt_at, last_indexed_at, last_error_code, last_error_redacted, updated_at)
        VALUES
            (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, NULL, NULL, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(path_hash) DO UPDATE SET
            provider = excluded.provider,
            basename = excluded.basename,
            redacted_display = excluded.redacted_display,
            file_size = excluded.file_size,
            file_mtime_ms = excluded.file_mtime_ms,
            content_hash = excluded.content_hash,
            state = excluded.state,
            remote_job_id = excluded.remote_job_id,
            failure_count = 0,
            next_attempt_at = NULL,
            last_indexed_at = NULL,
            last_error_code = NULL,
            last_error_redacted = NULL,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&meta.path_hash)
    .bind(&meta.provider)
    .bind(&meta.basename)
    .bind(&meta.redacted_display)
    .bind(meta.file_size as i64)
    .bind(meta.file_mtime_ms)
    .bind(content_hash)
    .bind(SessionWatchCheckpointState::RemoteAccepted.as_str())
    .bind(remote_job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn checkpoint_remote_accepted_exists_for_path_hash(
    pool: &SqlitePool,
    path_hash: &str,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM axon_session_watch_checkpoints
            WHERE path_hash = ?
              AND state = 'remote_accepted'
              AND remote_job_id IS NOT NULL
              AND last_error_code IS NULL
            LIMIT 1
        )
        "#,
    )
    .bind(path_hash)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn record_error(
    pool: &SqlitePool,
    meta: &SessionFileMetadata,
    error_code: &str,
    error_redacted: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO axon_session_watch_errors (path_hash, provider, basename, error_code, error_redacted)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&meta.path_hash)
    .bind(&meta.provider)
    .bind(&meta.basename)
    .bind(error_code)
    .bind(error_redacted)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO axon_session_watch_checkpoints
            (path_hash, provider, basename, redacted_display, file_size, file_mtime_ms, state, remote_job_id, last_error_code, last_error_redacted, failure_count, updated_at)
        VALUES
            (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(path_hash) DO UPDATE SET
            provider = excluded.provider,
            basename = excluded.basename,
            redacted_display = excluded.redacted_display,
            file_size = excluded.file_size,
            file_mtime_ms = excluded.file_mtime_ms,
            state = 'error',
            remote_job_id = NULL,
            last_error_code = excluded.last_error_code,
            last_error_redacted = excluded.last_error_redacted,
            failure_count = axon_session_watch_checkpoints.failure_count + 1,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&meta.path_hash)
    .bind(&meta.provider)
    .bind(&meta.basename)
    .bind(&meta.redacted_display)
    .bind(meta.file_size as i64)
    .bind(meta.file_mtime_ms)
    .bind(SessionWatchCheckpointState::Error.as_str())
    .bind(error_code)
    .bind(error_redacted)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_recent_errors(pool: &SqlitePool, limit: i64) -> Result<Vec<SessionWatchError>> {
    let rows = sqlx::query(
        r#"
        SELECT path_hash, provider, basename, error_code, error_redacted, occurred_at
        FROM axon_session_watch_errors
        ORDER BY id DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(session_watch_error_from_row).collect())
}

pub async fn watch_status(pool: &SqlitePool, limit: i64) -> Result<SessionWatchStatus> {
    let checkpoint_count = table_count(pool, "axon_session_watch_checkpoints").await?;
    let error_count = table_count(pool, "axon_session_watch_errors").await?;
    let recent_errors = list_recent_errors(pool, limit.max(0)).await?;
    Ok(SessionWatchStatus {
        checkpoint_count,
        error_count,
        recent_errors,
    })
}

pub async fn checkpoint_success_exists_for_path_hash(
    pool: &SqlitePool,
    path_hash: &str,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM axon_session_watch_checkpoints
            WHERE path_hash = ?
              AND last_error_code IS NULL
              AND last_indexed_at IS NOT NULL
              AND state IN ('local_ingested', 'no_content')
            LIMIT 1
        )
        "#,
    )
    .bind(path_hash)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

fn session_watch_error_from_row(row: SqliteRow) -> SessionWatchError {
    SessionWatchError {
        path_hash: row.get("path_hash"),
        provider: row.get("provider"),
        basename: row.get("basename"),
        error_code: row.get("error_code"),
        error_redacted: row.get("error_redacted"),
        occurred_at: row.get("occurred_at"),
    }
}

async fn table_count(pool: &SqlitePool, table: &'static str) -> Result<i64> {
    let query = format!("SELECT COUNT(*) FROM {table}");
    Ok(sqlx::query_scalar::<_, i64>(&query).fetch_one(pool).await?)
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
