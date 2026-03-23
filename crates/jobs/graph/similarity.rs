use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_warn;
use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::vector::ops::qdrant::qdrant_base;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityEdge {
    pub source_url: String,
    pub target_url: String,
    pub score: f32,
    pub target_source_type: String,
}

pub fn chunk_point_id(url: &str, chunk_index: usize) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{url}:{chunk_index}").as_bytes(),
    )
}

pub fn build_recommend_request(
    _collection: &str,
    url: &str,
    threshold: f64,
    limit: usize,
) -> Value {
    serde_json::json!({
        "query": {
            "recommend": {
                "positive": [chunk_point_id(url, 0).to_string()]
            }
        },
        "using": "dense",
        "limit": limit,
        "with_payload": true,
        "score_threshold": threshold,
        "filter": {
            "must_not": [
                {"key": "url", "match": {"value": url}}
            ]
        }
    })
}

pub fn group_by_url_max_score(results: Vec<(String, f32, String)>) -> Vec<SimilarityEdge> {
    let mut grouped: HashMap<String, SimilarityEdge> = HashMap::new();

    for (target_url, score, target_source_type) in results {
        let entry = grouped
            .entry(target_url.clone())
            .or_insert_with(|| SimilarityEdge {
                source_url: String::new(),
                target_url,
                score,
                target_source_type: target_source_type.clone(),
            });
        if score > entry.score {
            entry.score = score;
            entry.target_source_type = target_source_type;
        }
    }

    let mut edges: Vec<_> = grouped.into_values().collect();
    edges.sort_by(|left, right| left.target_url.cmp(&right.target_url));
    edges
}

pub async fn compute_similarity(
    cfg: &Config,
    neo4j: &Neo4jClient,
    url: &str,
) -> Result<Vec<SimilarityEdge>, Box<dyn std::error::Error>> {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );
    let http_response = client
        .post(&endpoint)
        .json(&build_recommend_request(
            &cfg.collection,
            url,
            cfg.graph_similarity_threshold,
            cfg.graph_similarity_limit,
        ))
        .send()
        .await?;

    if !http_response.status().is_success() {
        let status = http_response.status();
        let body = http_response.text().await.unwrap_or_default();
        log_warn(&format!(
            "compute_similarity: Qdrant returned {status} for url={url}: {body}"
        ));
        return Ok(vec![]);
    }

    let response = http_response.json::<Value>().await?;

    let raw_results = response["result"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let target_url = item["payload"]["url"].as_str()?.to_string();
            let score = item["score"].as_f64()? as f32;
            let source_type = item["payload"]["source_type"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            Some((target_url, score, source_type))
        })
        .collect::<Vec<_>>();

    let mut edges = group_by_url_max_score(raw_results);
    for edge in &mut edges {
        edge.source_url = url.to_string();
    }

    if !edges.is_empty() {
        let items: Vec<Value> = edges
            .iter()
            .map(|edge| {
                serde_json::json!({
                    "source_url": edge.source_url,
                    "target_url": edge.target_url,
                    "score": edge.score,
                    "target_source_type": edge.target_source_type,
                })
            })
            .collect();

        neo4j
            .execute(
                "UNWIND $items AS item \
                 MERGE (d1:Document {url: item.source_url}) \
                 MERGE (d2:Document {url: item.target_url}) \
                 MERGE (d1)-[r:SIMILAR_TO]->(d2) \
                 SET r.score = item.score, \
                     r.target_source_type = item.target_source_type, \
                     r.updated_at = datetime()",
                serde_json::json!({ "items": items }),
            )
            .await?;
    }

    Ok(edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_point_id_is_deterministic() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://tokio.rs/tutorial", 0);
        assert_eq!(id1, id2);
    }

    #[test]
    fn chunk_point_id_varies_by_url() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://axum.rs/tutorial", 0);
        assert_ne!(id1, id2);
    }

    #[test]
    fn chunk_point_id_varies_by_index() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://tokio.rs/tutorial", 1);
        assert_ne!(id1, id2);
    }

    #[test]
    fn build_recommend_request_structure() {
        let req = build_recommend_request("cortex", "https://example.com", 0.75, 20);
        assert_eq!(
            req["query"]["recommend"]["positive"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(req["filter"]["must_not"].as_array().unwrap().len(), 1);
        assert_eq!(
            req["using"], "dense",
            "named-vector collections require 'using' field"
        );
    }

    #[test]
    fn similarity_edge_construction() {
        let edge = SimilarityEdge {
            source_url: "https://a.com".to_string(),
            target_url: "https://b.com".to_string(),
            score: 0.87,
            target_source_type: "crawl".to_string(),
        };
        assert!(edge.score > 0.75);
    }

    #[test]
    fn group_results_by_url_takes_max_score() {
        let results = vec![
            ("https://b.com".to_string(), 0.82, "crawl".to_string()),
            ("https://b.com".to_string(), 0.91, "crawl".to_string()),
            ("https://c.com".to_string(), 0.78, "github".to_string()),
        ];
        let grouped = group_by_url_max_score(results);
        assert_eq!(grouped.len(), 2);
        let b = grouped
            .iter()
            .find(|edge| edge.target_url == "https://b.com")
            .unwrap();
        assert!((b.score - 0.91).abs() < f64::EPSILON as f32);
    }
}
