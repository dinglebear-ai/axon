//! Latest per-URL snapshot + HTTP validators (`axon_watch_url_state`).

use sqlx::SqlitePool;
use uuid::Uuid;

/// Cap on the stored snapshot markdown so an adversarially large watched page
/// cannot grow the row unbounded. Truncated on a UTF-8 char boundary.
pub const MAX_SNAPSHOT_MARKDOWN_BYTES: usize = 512 * 1024;

/// Truncate `s` to at most `MAX_SNAPSHOT_MARKDOWN_BYTES` on a char boundary.
pub fn truncate_snapshot_markdown(s: &str) -> String {
    if s.len() <= MAX_SNAPSHOT_MARKDOWN_BYTES {
        return s.to_string();
    }
    let mut end = MAX_SNAPSHOT_MARKDOWN_BYTES.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UrlState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_hash: Option<String>,
    pub last_markdown: Option<String>,
    pub last_links_json: Option<String>,
    pub last_checked_at: Option<i64>,
    pub last_changed_at: Option<i64>,
    pub last_crawl_job_id: Option<Uuid>,
}

type Row = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

pub async fn get_url_state(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
) -> Result<Option<UrlState>, sqlx::Error> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT etag, last_modified, content_hash, last_markdown, last_links_json, \
         last_checked_at, last_changed_at, last_crawl_job_id \
         FROM axon_watch_url_state WHERE watch_id = ? AND url = ?",
    )
    .bind(watch_id.to_string())
    .bind(url)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(
            etag,
            last_modified,
            content_hash,
            last_markdown,
            last_links_json,
            last_checked_at,
            last_changed_at,
            last_crawl_job_id,
        )| UrlState {
            etag,
            last_modified,
            content_hash,
            last_markdown,
            last_links_json,
            last_checked_at,
            last_changed_at,
            last_crawl_job_id: last_crawl_job_id.and_then(|r| Uuid::parse_str(&r).ok()),
        },
    ))
}

pub async fn upsert_url_state(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    s: &UrlState,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO axon_watch_url_state \
         (watch_id, url, etag, last_modified, content_hash, last_markdown, last_links_json, last_checked_at, last_changed_at, last_crawl_job_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(watch_id, url) DO UPDATE SET \
           etag=excluded.etag, last_modified=excluded.last_modified, content_hash=excluded.content_hash, \
           last_markdown=excluded.last_markdown, last_links_json=excluded.last_links_json, \
           last_checked_at=excluded.last_checked_at, last_changed_at=excluded.last_changed_at, \
           last_crawl_job_id=excluded.last_crawl_job_id",
    )
    .bind(watch_id.to_string()).bind(url)
    .bind(&s.etag).bind(&s.last_modified).bind(&s.content_hash)
    .bind(s.last_markdown.as_deref().map(truncate_snapshot_markdown))
    .bind(&s.last_links_json)
    .bind(s.last_checked_at).bind(s.last_changed_at)
    .bind(s.last_crawl_job_id.map(|i| i.to_string()))
    .execute(pool)
    .await?;
    Ok(())
}

/// Set just `last_crawl_job_id` for a `(watch_id, url)` row. Used after
/// dispatching a change-triggered crawl so the in-flight guard can find the
/// referencing crawl on the next tick — without clobbering the freshly-written
/// snapshot columns (only `last_crawl_job_id` is touched on conflict).
///
/// Implemented as an upsert rather than a bare `UPDATE`: a plain update silently
/// affects 0 rows if the snapshot row is missing, leaving the in-flight guard
/// blind and allowing duplicate crawls. Changed URLs are normally upserted
/// before dispatch so the row exists, but the upsert keeps the guard reliable
/// even when it doesn't. A minimal row (all snapshot columns NULL) is inserted
/// in that case; the next `detect_url_change` tick fills it in.
pub async fn set_crawl_job_id(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    job_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO axon_watch_url_state (watch_id, url, last_crawl_job_id) \
         VALUES (?, ?, ?) \
         ON CONFLICT(watch_id, url) DO UPDATE SET last_crawl_job_id = excluded.last_crawl_job_id",
    )
    .bind(watch_id.to_string())
    .bind(url)
    .bind(job_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "url_state_tests.rs"]
mod tests;
