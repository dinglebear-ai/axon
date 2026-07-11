//! Qdrant collection statistics.
//!
//! Ports legacy `axon-vector`'s `ops::stats::stats_payload` (data gathering)
//! onto `axon-vectors`' read primitives — see [`stats_payload`] and the
//! [`qdrant_fetch`] submodule for what moved where and why. The SQLite
//! job-metrics gathering ([`sqlite`]) and the human-readable presentation
//! ([`display`], driving [`print_stats_human`]) never touched Qdrant at all,
//! so those ported over verbatim.

mod display;
mod qdrant_fetch;
mod sqlite;

use crate::types::StatsResult;
use axon_core::config::Config;
use axon_vectors::qdrant::QdrantVectorStore;
use std::error::Error;

pub fn map_stats_payload(payload: serde_json::Value) -> StatsResult {
    StatsResult { payload }
}

pub fn print_stats_human(stats: &serde_json::Value) {
    display::print_stats_human(stats);
}

#[must_use = "stats returns a Result that should be handled"]
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>> {
    let payload = stats_payload(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("stats query failed: {e}").into() })?;
    Ok(map_stats_payload(payload))
}

/// Gather every field `axon stats` reports: Qdrant collection info/point
/// counts ([`qdrant_fetch::fetch_qdrant_snapshots`]), an indexed-token-size
/// sample ([`qdrant_fetch::sample_indexed_token_stats`]), and SQLite job
/// metrics ([`sqlite::collect_job_metrics`]). Ports legacy `stats_payload`.
async fn stats_payload(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let (info, points_count, docs_embedded) =
        qdrant_fetch::fetch_qdrant_snapshots(cfg, &store).await?;

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
    let job_metrics = sqlite::collect_job_metrics(cfg).await;
    let indexed_token_stats = match qdrant_fetch::sample_indexed_token_stats(cfg, &store).await {
        Ok(stats) => stats,
        Err(e) => {
            tracing::warn!(error = %e, "stats: failed to sample indexed token stats");
            None
        }
    };
    let avg_chunk_tokens_estimate = indexed_token_stats
        .as_ref()
        .map(|stats| stats.avg_chunk_tokens_estimate)
        .or(job_metrics.avg_chunk_tokens_estimate);
    let avg_doc_tokens_estimate = indexed_token_stats
        .as_ref()
        .map(|stats| stats.avg_doc_tokens_estimate)
        .or(job_metrics.avg_doc_tokens_estimate);
    let indexed_token_stats_json = indexed_token_stats.as_ref().map(|stats| {
        serde_json::json!({
            "sampled_points": stats.sampled_points,
            "sampled_docs": stats.sampled_docs,
            "sample_limit_points": stats.sample_limit_points,
            "avg_chunk_chars": stats.avg_chunk_chars,
            "avg_chunk_tokens_estimate": stats.avg_chunk_tokens_estimate,
            "avg_doc_chars": stats.avg_doc_chars,
            "avg_doc_tokens_estimate": stats.avg_doc_tokens_estimate,
        })
    });

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
        "avg_pages_crawled_per_second": job_metrics.average_pages_per_second,
        "avg_crawl_duration_seconds": job_metrics.average_crawl_duration_seconds,
        "avg_embedding_duration_seconds": job_metrics.average_embedding_duration_seconds,
        "avg_overall_crawl_duration_seconds": job_metrics.average_overall_crawl_duration_seconds,
        "longest_crawl": job_metrics.longest_crawl,
        "most_chunks": job_metrics.most_chunks,
        "total_chunks": job_metrics.total_chunks,
        "total_docs": job_metrics.total_docs,
        "avg_chunk_tokens_estimate": avg_chunk_tokens_estimate,
        "avg_doc_tokens_estimate": avg_doc_tokens_estimate,
        "indexed_token_stats": indexed_token_stats_json,
        "base_urls_count": job_metrics.base_urls_count,
        "freshness": {
            "last_indexed_secs_ago": job_metrics.last_indexed_secs_ago,
            "crawls_last_24h": job_metrics.crawls_last_24h,
            "crawls_last_7d": job_metrics.crawls_last_7d,
        },
        "growth_7d": job_metrics.chunks_per_day_7d,
        "counts": {
            "crawls": job_metrics.crawl_count,
            "embeds": job_metrics.embed_count,
            "scrapes": job_metrics.scrape_count,
            "extracts": job_metrics.extract_count,
            "queries": job_metrics.query_count,
            "asks": job_metrics.ask_count,
            "retrieves": job_metrics.retrieve_count,
            "evaluates": job_metrics.evaluate_count,
            "suggests": job_metrics.suggest_count,
            "maps": job_metrics.map_count,
            "searches": job_metrics.search_count
        }
    }))
}
