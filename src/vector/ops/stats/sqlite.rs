use crate::core::config::Config;
use crate::jobs::store::open_sqlite_pool;
use sqlx::SqlitePool;

const ESTIMATED_CHARS_PER_CHUNK: f64 = 2000.0;
const ESTIMATED_CHARS_PER_TOKEN: f64 = 4.0;

#[derive(Default)]
pub(super) struct JobMetrics {
    pub(super) crawl_count: Option<i64>,
    pub(super) extract_count: Option<i64>,
    pub(super) average_pages_per_second: Option<f64>,
    pub(super) average_crawl_duration_seconds: Option<f64>,
    pub(super) average_embedding_duration_seconds: Option<f64>,
    pub(super) average_overall_crawl_duration_seconds: Option<f64>,
    pub(super) longest_crawl: Option<serde_json::Value>,
    pub(super) most_chunks: Option<serde_json::Value>,
    pub(super) total_chunks: Option<i64>,
    pub(super) total_docs: Option<i64>,
    pub(super) avg_chunk_tokens_estimate: Option<f64>,
    pub(super) avg_doc_tokens_estimate: Option<f64>,
    pub(super) base_urls_count: Option<i64>,
    pub(super) scrape_count: Option<i64>,
    pub(super) query_count: Option<i64>,
    pub(super) ask_count: Option<i64>,
    pub(super) retrieve_count: Option<i64>,
    pub(super) map_count: Option<i64>,
    pub(super) search_count: Option<i64>,
    pub(super) embed_count: Option<i64>,
    pub(super) evaluate_count: Option<i64>,
    pub(super) suggest_count: Option<i64>,
    // Temporal / freshness metrics
    pub(super) last_indexed_secs_ago: Option<i64>,
    pub(super) crawls_last_24h: Option<i64>,
    pub(super) crawls_last_7d: Option<i64>,
    pub(super) chunks_per_day_7d: Vec<serde_json::Value>,
}

/// Pull metrics from the SQLite job tables in the SQLite runtime.
///
/// Many fields (scrape/query/ask/retrieve/evaluate/suggest/map/search counts,
/// growth_7d) intentionally remain `None` — these were tracked in the old
/// Postgres command log, which no longer exists. The display falls back to
/// "n/a" for absent fields.
///
/// Any SQLite error returns an empty struct rather than propagating, so a
/// stats failure cannot make `axon stats` fail.
pub(super) async fn collect_job_metrics(cfg: &Config) -> JobMetrics {
    let path = cfg.sqlite_path.to_string_lossy().to_string();
    let pool = match open_sqlite_pool(&path).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, sqlite_path = %path, "stats: failed to open SQLite pool");
            return JobMetrics::default();
        }
    };
    collect_sqlite_metrics(&pool).await
}

async fn collect_sqlite_metrics(pool: &SqlitePool) -> JobMetrics {
    let mut m = JobMetrics::default();

    m.crawl_count = count_completed(pool, "axon_crawl_jobs").await;
    m.embed_count = count_completed(pool, "axon_embed_jobs").await;
    m.extract_count = count_completed(pool, "axon_extract_jobs").await;

    m.average_crawl_duration_seconds = avg_duration_secs(pool, "axon_crawl_jobs").await;
    m.average_embedding_duration_seconds = avg_duration_secs(pool, "axon_embed_jobs").await;
    m.average_overall_crawl_duration_seconds = m.average_crawl_duration_seconds;

    m.average_pages_per_second = avg_pages_per_second(pool).await;

    m.last_indexed_secs_ago = last_indexed_secs_ago(pool).await;
    m.crawls_last_24h = recent_completed(pool, "axon_crawl_jobs", 86_400_000).await;
    m.crawls_last_7d = recent_completed(pool, "axon_crawl_jobs", 7 * 86_400_000).await;

    let (total_docs, total_chunks) = embed_totals(pool).await;
    m.total_docs = total_docs;
    m.total_chunks = total_chunks;
    m.avg_chunk_tokens_estimate = Some(estimated_avg_chunk_tokens());
    m.avg_doc_tokens_estimate = estimated_avg_doc_tokens(total_docs, total_chunks);
    m.most_chunks = most_chunks_job(pool).await;
    m.longest_crawl = longest_crawl_job(pool).await;

    m
}

pub(super) fn estimated_avg_chunk_tokens() -> f64 {
    ESTIMATED_CHARS_PER_CHUNK / ESTIMATED_CHARS_PER_TOKEN
}

pub(super) fn estimated_avg_doc_tokens(
    total_docs: Option<i64>,
    total_chunks: Option<i64>,
) -> Option<f64> {
    let docs = total_docs?;
    if docs <= 0 {
        return None;
    }
    let chunks = total_chunks?;
    Some((chunks as f64 / docs as f64) * estimated_avg_chunk_tokens())
}

async fn count_completed(pool: &SqlitePool, table: &str) -> Option<i64> {
    let q = format!("SELECT COUNT(*) FROM {table} WHERE status='completed'");
    sqlx::query_scalar::<_, i64>(&q).fetch_one(pool).await.ok()
}

async fn avg_duration_secs(pool: &SqlitePool, table: &str) -> Option<f64> {
    let q = format!(
        "SELECT AVG((finished_at - started_at) / 1000.0) \
         FROM {table} \
         WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL"
    );
    sqlx::query_scalar::<_, Option<f64>>(&q)
        .fetch_one(pool)
        .await
        .ok()
        .flatten()
}

async fn avg_pages_per_second(pool: &SqlitePool) -> Option<f64> {
    // result_json is a TEXT column; SQLite ships json_extract for parsing it.
    let q = "SELECT AVG( \
                CAST(json_extract(result_json, '$.pages_crawled') AS REAL) \
                / NULLIF(CAST(json_extract(result_json, '$.elapsed_ms') AS REAL) / 1000.0, 0) \
              ) \
              FROM axon_crawl_jobs \
              WHERE status='completed' \
                AND result_json IS NOT NULL \
                AND json_extract(result_json, '$.pages_crawled') IS NOT NULL \
                AND json_extract(result_json, '$.elapsed_ms') IS NOT NULL";
    sqlx::query_scalar::<_, Option<f64>>(q)
        .fetch_one(pool)
        .await
        .ok()
        .flatten()
}

async fn last_indexed_secs_ago(pool: &SqlitePool) -> Option<i64> {
    let q = "SELECT MAX(finished_at) FROM axon_embed_jobs WHERE status='completed'";
    let max_ms: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(q)
        .fetch_one(pool)
        .await
        .ok()
        .flatten();
    max_ms.map(|ms| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(ms);
        ((now - ms) / 1000).max(0)
    })
}

async fn recent_completed(pool: &SqlitePool, table: &str, window_ms: i64) -> Option<i64> {
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64
        - window_ms;
    let q = format!("SELECT COUNT(*) FROM {table} WHERE status='completed' AND finished_at >= ?");
    sqlx::query_scalar::<_, i64>(&q)
        .bind(cutoff)
        .fetch_one(pool)
        .await
        .ok()
}

async fn embed_totals(pool: &SqlitePool) -> (Option<i64>, Option<i64>) {
    let q = "SELECT \
                COALESCE(SUM(CAST(json_extract(result_json, '$.docs_embedded') AS INTEGER)), 0), \
                COALESCE(SUM(CAST(json_extract(result_json, '$.chunks_embedded') AS INTEGER)), 0) \
              FROM axon_embed_jobs \
              WHERE status='completed' AND result_json IS NOT NULL";
    match sqlx::query_as::<_, (i64, i64)>(q).fetch_one(pool).await {
        Ok((d, c)) => (Some(d), Some(c)),
        Err(_) => (None, None),
    }
}

async fn most_chunks_job(pool: &SqlitePool) -> Option<serde_json::Value> {
    let q = "SELECT id, CAST(json_extract(result_json, '$.chunks_embedded') AS INTEGER) AS chunks \
              FROM axon_embed_jobs \
              WHERE status='completed' AND result_json IS NOT NULL \
              ORDER BY chunks DESC NULLS LAST \
              LIMIT 1";
    let row: Option<(String, Option<i64>)> =
        sqlx::query_as(q).fetch_optional(pool).await.ok().flatten();
    row.and_then(|(id, chunks)| {
        chunks.map(|c| serde_json::json!({"embed_job_id": id, "chunks": c}))
    })
}

async fn longest_crawl_job(pool: &SqlitePool) -> Option<serde_json::Value> {
    let q = "SELECT id, (finished_at - started_at) / 1000.0 AS secs \
              FROM axon_crawl_jobs \
              WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL \
              ORDER BY secs DESC \
              LIMIT 1";
    let row: Option<(String, Option<f64>)> =
        sqlx::query_as(q).fetch_optional(pool).await.ok().flatten();
    row.and_then(|(id, secs)| secs.map(|s| serde_json::json!({"id": id, "seconds": s})))
}

#[cfg(test)]
mod tests {
    use super::{estimated_avg_chunk_tokens, estimated_avg_doc_tokens};

    #[test]
    fn estimates_token_averages_from_chunk_and_doc_totals() {
        assert_eq!(estimated_avg_chunk_tokens(), 500.0);
        assert_eq!(estimated_avg_doc_tokens(Some(4), Some(40)), Some(5_000.0));
    }

    #[test]
    fn token_average_is_absent_without_docs() {
        assert_eq!(estimated_avg_doc_tokens(Some(0), Some(40)), None);
        assert_eq!(estimated_avg_doc_tokens(None, Some(40)), None);
    }
}
