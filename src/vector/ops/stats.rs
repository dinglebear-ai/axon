pub(crate) mod display;
mod pg;
mod qdrant_fetch;

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use std::error::Error;

pub async fn stats_payload(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let client = internal_service_http_client()?;
    let (info, count, docs_count) = qdrant_fetch::fetch_qdrant_snapshots(cfg, client).await?;

    let points_count = count["result"]["count"].as_u64().unwrap_or(0);
    let docs_embedded = docs_count["result"]["count"].as_u64().unwrap_or(0);
    let avg_chunks_per_doc = if docs_embedded > 0 {
        points_count as f64 / docs_embedded as f64
    } else {
        0.0
    };
    let indexed_vectors = info["result"]["indexed_vectors_count"]
        .as_u64()
        .or_else(|| info["result"]["vectors_count"].as_u64());
    let segments_count = info["result"]["segments_count"].as_u64();
    let payload_schema = info["result"]["payload_schema"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let payload_fields: Vec<String> = payload_schema.keys().cloned().collect();
    let payload_fields_count = payload_fields.len();
    let pg = pg::collect_postgres_metrics(cfg).await;

    Ok(serde_json::json!({
        "collection": cfg.collection,
        "status": info["result"]["status"],
        "indexed_vectors_count": indexed_vectors,
        "points_count": points_count,
        "dimension": info["result"]["config"]["params"]["vectors"]["size"],
        "distance": info["result"]["config"]["params"]["vectors"]["distance"],
        "segments_count": segments_count,
        "docs_embedded_estimate": docs_embedded,
        "avg_chunks_per_doc": avg_chunks_per_doc,
        "payload_fields_count": payload_fields_count,
        "payload_fields": payload_fields,
        "avg_pages_crawled_per_second": pg.average_pages_per_second,
        "avg_crawl_duration_seconds": pg.average_crawl_duration_seconds,
        "avg_embedding_duration_seconds": pg.average_embedding_duration_seconds,
        "avg_overall_crawl_duration_seconds": pg.average_overall_crawl_duration_seconds,
        "longest_crawl": pg.longest_crawl,
        "most_chunks": pg.most_chunks,
        "total_chunks": pg.total_chunks,
        "total_docs": pg.total_docs,
        "base_urls_count": pg.base_urls_count,
        "freshness": {
            "last_indexed_secs_ago": pg.last_indexed_secs_ago,
            "crawls_last_24h": pg.crawls_last_24h,
            "crawls_last_7d": pg.crawls_last_7d,
        },
        "growth_7d": pg.chunks_per_day_7d,
        "counts": {
            "crawls": pg.crawl_count,
            "embeds": pg.embed_count,
            "scrapes": pg.scrape_count,
            "extracts": pg.extract_count,
            "queries": pg.query_count,
            "asks": pg.ask_count,
            "retrieves": pg.retrieve_count,
            "evaluates": pg.evaluate_count,
            "suggests": pg.suggest_count,
            "maps": pg.map_count,
            "searches": pg.search_count
        }
    }))
}
