use crate::crates::core::config::Config;
use futures_util::future;
use sqlx::{Row, postgres::PgPoolOptions};
use std::time::Duration;

/// Whitelist of tables allowed in dynamic SQL queries.
/// Prevents SQL injection by ensuring only known table names are interpolated.
#[derive(Clone, Copy)]
pub(super) enum StatsTable {
    Crawl,
    Extract,
    Embed,
}

impl StatsTable {
    fn as_str(self) -> &'static str {
        match self {
            Self::Crawl => "axon_crawl_jobs",
            Self::Extract => "axon_extract_jobs",
            Self::Embed => "axon_embed_jobs",
        }
    }
}

pub(super) async fn pg_pool_for_stats(cfg: &Config) -> Option<sqlx::PgPool> {
    if cfg.pg_url.is_empty() {
        return None;
    }
    tokio::time::timeout(
        Duration::from_secs(3),
        PgPoolOptions::new().max_connections(2).connect(&cfg.pg_url),
    )
    .await
    .ok()
    .and_then(Result::ok)
}

async fn table_exists(pool: &sqlx::PgPool, table: StatsTable) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NOT NULL")
        .bind(table.as_str())
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

async fn count_table_rows(pool: &sqlx::PgPool, table: StatsTable) -> Option<i64> {
    let sql = format!("SELECT COUNT(*) FROM {}", table.as_str());
    sqlx::query_scalar::<_, i64>(&sql)
        .fetch_one(pool)
        .await
        .ok()
}

async fn command_count(pool: &sqlx::PgPool, command: &str) -> Option<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM axon_command_runs WHERE command = $1")
        .bind(command)
        .fetch_one(pool)
        .await
        .ok()
}

#[derive(Default)]
pub(super) struct PostgresMetrics {
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

pub(super) async fn collect_postgres_metrics(cfg: &Config) -> PostgresMetrics {
    let Some(pool) = pg_pool_for_stats(cfg).await else {
        return PostgresMetrics::default();
    };

    // Check all table existence in parallel.
    let (crawl_exists, extract_exists, embed_exists, runs_exists) = tokio::join!(
        table_exists(&pool, StatsTable::Crawl),
        table_exists(&pool, StatsTable::Extract),
        table_exists(&pool, StatsTable::Embed),
        // axon_command_runs is not a job table — use parameterized query directly.
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('axon_command_runs') IS NOT NULL")
            .fetch_one(&pool),
    );
    let runs_exists = runs_exists.unwrap_or(false);

    // Collect all metric groups in parallel.
    let (crawl_m, extract_m, embed_m, cmd_m) = tokio::join!(
        async {
            if crawl_exists {
                collect_crawl_metrics(&pool).await
            } else {
                PostgresMetrics::default()
            }
        },
        async {
            if extract_exists {
                collect_extract_metrics(&pool).await
            } else {
                PostgresMetrics::default()
            }
        },
        async {
            if embed_exists {
                collect_embed_metrics(&pool).await
            } else {
                PostgresMetrics::default()
            }
        },
        async {
            if runs_exists {
                collect_command_metrics(&pool).await
            } else {
                PostgresMetrics::default()
            }
        },
    );

    // Merge disjoint partial results.
    PostgresMetrics {
        crawl_count: crawl_m.crawl_count,
        base_urls_count: crawl_m.base_urls_count,
        average_pages_per_second: crawl_m.average_pages_per_second,
        average_crawl_duration_seconds: crawl_m.average_crawl_duration_seconds,
        longest_crawl: crawl_m.longest_crawl,
        average_overall_crawl_duration_seconds: crawl_m.average_overall_crawl_duration_seconds,
        crawls_last_24h: crawl_m.crawls_last_24h,
        crawls_last_7d: crawl_m.crawls_last_7d,
        extract_count: extract_m.extract_count,
        average_embedding_duration_seconds: embed_m.average_embedding_duration_seconds,
        total_chunks: embed_m.total_chunks,
        total_docs: embed_m.total_docs,
        embed_count: embed_m.embed_count,
        most_chunks: embed_m.most_chunks,
        last_indexed_secs_ago: embed_m.last_indexed_secs_ago,
        chunks_per_day_7d: embed_m.chunks_per_day_7d,
        scrape_count: cmd_m.scrape_count,
        query_count: cmd_m.query_count,
        ask_count: cmd_m.ask_count,
        retrieve_count: cmd_m.retrieve_count,
        map_count: cmd_m.map_count,
        search_count: cmd_m.search_count,
        evaluate_count: cmd_m.evaluate_count,
        suggest_count: cmd_m.suggest_count,
    }
}

async fn collect_crawl_metrics(pool: &sqlx::PgPool) -> PostgresMetrics {
    let (count, base_urls, pages_per_sec, crawl_dur, overall_dur, longest, count_24h, count_7d) =
        tokio::join!(
            count_table_rows(pool, StatsTable::Crawl),
            sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT url) FROM axon_crawl_jobs")
                .fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>(
                r#"
                SELECT AVG(
                    COALESCE((result_json->>'pages_discovered')::double precision, 0.0)
                    / GREATEST(EXTRACT(EPOCH FROM (finished_at - started_at))::double precision, 0.001::double precision)
                )
                FROM axon_crawl_jobs
                WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL
                "#,
            )
            .fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(EXTRACT(EPOCH FROM (finished_at - started_at))::double precision) FROM axon_crawl_jobs WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL",
            )
            .fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>(
                r#"
                SELECT AVG(
                    EXTRACT(EPOCH FROM (
                        COALESCE(e.finished_at, c.finished_at) - c.started_at
                    ))::double precision
                )
                FROM axon_crawl_jobs c
                LEFT JOIN LATERAL (
                    SELECT finished_at
                    FROM axon_embed_jobs e
                    WHERE e.status='completed'
                      AND e.input_text LIKE ('%' || c.id::text || '/markdown')
                    ORDER BY finished_at DESC
                    LIMIT 1
                ) e ON TRUE
                WHERE c.status='completed' AND c.started_at IS NOT NULL AND c.finished_at IS NOT NULL
                "#,
            )
            .fetch_one(pool),
            sqlx::query(
                r#"
                SELECT id::text AS id, url, EXTRACT(EPOCH FROM (finished_at - started_at))::double precision AS seconds
                FROM axon_crawl_jobs
                WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL
                ORDER BY (finished_at - started_at) DESC
                LIMIT 1
                "#,
            )
            .fetch_optional(pool),
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM axon_crawl_jobs WHERE status='completed' AND finished_at > NOW() - INTERVAL '24 hours'",
            )
            .fetch_one(pool),
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM axon_crawl_jobs WHERE status='completed' AND finished_at > NOW() - INTERVAL '7 days'",
            )
            .fetch_one(pool),
        );

    let longest_crawl = longest.ok().flatten().map(|row| {
        let id: String = row.get("id");
        let url: String = row.get("url");
        let seconds: f64 = row.get("seconds");
        serde_json::json!({"id": id, "url": url, "seconds": seconds})
    });

    PostgresMetrics {
        crawl_count: count,
        base_urls_count: base_urls.ok(),
        average_pages_per_second: pages_per_sec.ok().flatten(),
        average_crawl_duration_seconds: crawl_dur.ok().flatten(),
        average_overall_crawl_duration_seconds: overall_dur.ok().flatten(),
        longest_crawl,
        crawls_last_24h: count_24h.ok(),
        crawls_last_7d: count_7d.ok(),
        ..Default::default()
    }
}

async fn collect_extract_metrics(pool: &sqlx::PgPool) -> PostgresMetrics {
    PostgresMetrics {
        extract_count: count_table_rows(pool, StatsTable::Extract).await,
        ..Default::default()
    }
}

async fn collect_embed_metrics(pool: &sqlx::PgPool) -> PostgresMetrics {
    let (avg_dur, total_chunks, total_docs, embed_count, most_chunks_row, last_indexed, growth_rows) =
        tokio::join!(
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(EXTRACT(EPOCH FROM (finished_at - started_at))::double precision) FROM axon_embed_jobs WHERE status='completed' AND started_at IS NOT NULL AND finished_at IS NOT NULL",
            )
            .fetch_one(pool),
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT SUM(COALESCE((result_json->>'chunks_embedded')::bigint, 0))::bigint FROM axon_embed_jobs WHERE status='completed'",
            )
            .fetch_one(pool),
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT SUM(COALESCE((result_json->>'docs_embedded')::bigint, 0))::bigint FROM axon_embed_jobs WHERE status='completed'",
            )
            .fetch_one(pool),
            count_table_rows(pool, StatsTable::Embed),
            sqlx::query(
                r#"
                SELECT id::text AS id,
                       COALESCE((result_json->>'chunks_embedded')::bigint, 0) AS chunks
                FROM axon_embed_jobs
                WHERE status='completed'
                ORDER BY COALESCE((result_json->>'chunks_embedded')::bigint, 0) DESC
                LIMIT 1
                "#,
            )
            .fetch_optional(pool),
            // Seconds since the most recent completed embed job finished.
            // Returns NULL (→ None) when no completed embed jobs exist.
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT EXTRACT(EPOCH FROM (NOW() - MAX(finished_at)))::bigint FROM axon_embed_jobs WHERE status='completed' AND finished_at IS NOT NULL",
            )
            .fetch_one(pool),
            // Per-day chunk counts for the last 7 days — drives the growth trend display.
            sqlx::query(
                r#"
                SELECT DATE(finished_at AT TIME ZONE 'UTC')::text AS day,
                       SUM(COALESCE((result_json->>'chunks_embedded')::bigint, 0))::bigint AS chunks
                FROM axon_embed_jobs
                WHERE status='completed' AND finished_at > NOW() - INTERVAL '7 days'
                GROUP BY DATE(finished_at AT TIME ZONE 'UTC')
                ORDER BY day ASC
                "#,
            )
            .fetch_all(pool),
        );

    let most_chunks = most_chunks_row.ok().flatten().map(|row| {
        let id: String = row.get("id");
        let chunks: i64 = row.get("chunks");
        serde_json::json!({"embed_job_id": id, "chunks": chunks})
    });

    let chunks_per_day_7d = growth_rows
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let day: String = row.get("day");
            let chunks: i64 = row.get("chunks");
            serde_json::json!({"date": day, "chunks": chunks})
        })
        .collect();

    PostgresMetrics {
        average_embedding_duration_seconds: avg_dur.ok().flatten(),
        total_chunks: total_chunks.ok().flatten(),
        total_docs: total_docs.ok().flatten(),
        embed_count,
        most_chunks,
        last_indexed_secs_ago: last_indexed.ok().flatten(),
        chunks_per_day_7d,
        ..Default::default()
    }
}

async fn collect_command_metrics(pool: &sqlx::PgPool) -> PostgresMetrics {
    // Run all 8 count queries in parallel.
    let commands = [
        "scrape", "query", "ask", "retrieve", "map", "search", "evaluate", "suggest",
    ];
    let counts: Vec<Option<i64>> =
        future::join_all(commands.iter().map(|cmd| command_count(pool, cmd))).await;
    let [scrape, query, ask, retrieve, map, search, evaluate, suggest] =
        counts.try_into().unwrap_or_default();

    PostgresMetrics {
        scrape_count: scrape,
        query_count: query,
        ask_count: ask,
        retrieve_count: retrieve,
        map_count: map,
        search_count: search,
        evaluate_count: evaluate,
        suggest_count: suggest,
        ..Default::default()
    }
}
