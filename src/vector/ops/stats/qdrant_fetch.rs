use crate::core::config::Config;
use crate::vector::ops::qdrant::{qdrant_base, qdrant_scroll_pages_selective};
use std::collections::HashMap;
use std::error::Error;

const TOKEN_STATS_SAMPLE_POINTS: usize = 5_000;
const CHARS_PER_TOKEN_ESTIMATE: f64 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct IndexedTokenStats {
    pub sampled_points: usize,
    pub sampled_docs: usize,
    pub sample_limit_points: usize,
    pub avg_chunk_chars: f64,
    pub avg_chunk_tokens_estimate: f64,
    pub avg_doc_chars: f64,
    pub avg_doc_tokens_estimate: f64,
}

pub(super) async fn fetch_qdrant_snapshots(
    cfg: &Config,
    client: &reqwest::Client,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let base = qdrant_base(cfg);
    let col = &cfg.collection;

    // All three requests are independent — run concurrently with tokio::join!
    // to eliminate serial round-trip latency (saves 10-30ms per stats call).
    let (info_res, count_res, docs_count_res) = tokio::join!(
        async {
            client
                .get(format!("{base}/collections/{col}"))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
        async {
            client
                .post(format!("{base}/collections/{col}/points/count"))
                .json(&serde_json::json!({"exact": false}))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
        async {
            client
                .post(format!("{base}/collections/{col}/points/count"))
                .json(&serde_json::json!({
                    "exact": false,
                    "filter": {"must": [{"key": "chunk_index", "match": { "value": 0 }}]}
                }))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
    );

    Ok((info_res?, count_res?, docs_count_res?))
}

pub(super) async fn sample_indexed_token_stats(
    cfg: &Config,
) -> Result<Option<IndexedTokenStats>, Box<dyn Error>> {
    let mut sampled_points = 0usize;
    let mut total_chunk_chars = 0usize;
    let mut doc_chars: HashMap<String, usize> = HashMap::new();

    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["url", "chunk_text", "text"]}),
        |points| {
            for point in points {
                if sampled_points >= TOKEN_STATS_SAMPLE_POINTS {
                    return false;
                }
                let Some(payload) = point.get("payload").and_then(|v| v.as_object()) else {
                    continue;
                };
                let Some(url) = payload.get("url").and_then(|v| v.as_str()) else {
                    continue;
                };
                let text = payload
                    .get("chunk_text")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .or_else(|| payload.get("text").and_then(|v| v.as_str()))
                    .unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                let chars = text.chars().count();
                sampled_points += 1;
                total_chunk_chars += chars;
                *doc_chars.entry(url.to_string()).or_default() += chars;
            }
            sampled_points < TOKEN_STATS_SAMPLE_POINTS
        },
    )
    .await?;

    Ok(indexed_token_stats_from_totals(
        sampled_points,
        total_chunk_chars,
        doc_chars,
        TOKEN_STATS_SAMPLE_POINTS,
    ))
}

fn indexed_token_stats_from_totals(
    sampled_points: usize,
    total_chunk_chars: usize,
    doc_chars: HashMap<String, usize>,
    sample_limit_points: usize,
) -> Option<IndexedTokenStats> {
    if sampled_points == 0 || doc_chars.is_empty() {
        return None;
    }
    let sampled_docs = doc_chars.len();
    let total_doc_chars = doc_chars.values().sum::<usize>();
    let avg_chunk_chars = total_chunk_chars as f64 / sampled_points as f64;
    let avg_doc_chars = total_doc_chars as f64 / sampled_docs as f64;
    Some(IndexedTokenStats {
        sampled_points,
        sampled_docs,
        sample_limit_points,
        avg_chunk_chars,
        avg_chunk_tokens_estimate: avg_chunk_chars / CHARS_PER_TOKEN_ESTIMATE,
        avg_doc_chars,
        avg_doc_tokens_estimate: avg_doc_chars / CHARS_PER_TOKEN_ESTIMATE,
    })
}

#[cfg(test)]
mod tests {
    use super::indexed_token_stats_from_totals;
    use std::collections::HashMap;

    #[test]
    fn indexed_token_stats_average_chunks_and_docs() {
        let doc_chars = HashMap::from([
            ("https://a.example".to_string(), 4_000usize),
            ("https://b.example".to_string(), 2_000usize),
        ]);
        let stats = indexed_token_stats_from_totals(4, 6_000, doc_chars, 5_000).unwrap();
        assert_eq!(stats.sampled_points, 4);
        assert_eq!(stats.sampled_docs, 2);
        assert_eq!(stats.avg_chunk_chars, 1_500.0);
        assert_eq!(stats.avg_chunk_tokens_estimate, 375.0);
        assert_eq!(stats.avg_doc_chars, 3_000.0);
        assert_eq!(stats.avg_doc_tokens_estimate, 750.0);
    }

    #[test]
    fn indexed_token_stats_absent_without_samples() {
        assert!(indexed_token_stats_from_totals(0, 0, HashMap::new(), 5_000).is_none());
    }
}
