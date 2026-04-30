use crate::crates::core::config::Config;

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

/// Postgres metrics are unavailable in the simplified build (no Postgres connection).
/// Returns a default empty struct so callers do not need to change.
pub(super) async fn collect_postgres_metrics(_cfg: &Config) -> PostgresMetrics {
    PostgresMetrics::default()
}
