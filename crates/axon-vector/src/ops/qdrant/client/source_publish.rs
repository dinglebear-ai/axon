use std::collections::BTreeSet;
use std::time::Instant;

use anyhow::{Result, anyhow};
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use serde_json::Value;

use super::super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceGenerationPointCounts {
    pub points: usize,
    pub distinct_items: usize,
}

pub async fn qdrant_publish_source_generation(
    cfg: &Config,
    source_id: &str,
    source_generation: i64,
    source_index_version: i64,
    expected_visible_points: usize,
) -> Result<()> {
    if source_id.trim().is_empty() {
        return Err(anyhow!("source publish source_id cannot be empty"));
    }
    if source_generation <= 0 {
        return Err(anyhow!("source publish generation must be positive"));
    }
    if source_index_version <= 0 {
        return Err(anyhow!("source publish index_version must be positive"));
    }
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/payload?wait=true")?;
    let body = serde_json::json!({
        "payload": {
            "source_committed": true
        },
        "filter": {
            "must": [
                {"key": "source_id", "match": {"value": source_id}},
                {"key": "source_generation", "match": {"value": source_generation}},
                {"key": "source_index_version", "match": {"value": source_index_version}}
            ]
        }
    });
    let _: Value = qdrant_post_json_with_retry(
        client,
        &endpoint,
        &body,
        "qdrant_publish_source_generation",
        &cfg.collection,
        Instant::now(),
    )
    .await?;
    if expected_visible_points > 0 {
        let count = qdrant_count_published_source_generation(
            cfg,
            source_id,
            source_generation,
            source_index_version,
        )
        .await?;
        if count < expected_visible_points as u64 {
            return Err(anyhow!(
                "source generation publish verified {count} visible points, expected at least {expected_visible_points}"
            ));
        }
    }
    Ok(())
}

pub async fn qdrant_source_generation_point_counts(
    cfg: &Config,
    source_id: &str,
    source_generation: i64,
    source_index_version: i64,
) -> Result<SourceGenerationPointCounts> {
    if source_id.trim().is_empty() {
        return Err(anyhow!("source generation count source_id cannot be empty"));
    }
    if source_generation <= 0 {
        return Err(anyhow!(
            "source generation count generation must be positive"
        ));
    }
    if source_index_version <= 0 {
        return Err(anyhow!(
            "source generation count index_version must be positive"
        ));
    }

    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/scroll")?;
    let mut offset = None;
    let mut points = 0_usize;
    let mut item_keys = BTreeSet::new();

    loop {
        let mut body = serde_json::json!({
            "limit": 1024,
            "with_payload": ["source_item_key"],
            "with_vector": false,
            "filter": {
                "must": [
                    {"key": "source_id", "match": {"value": source_id}},
                    {"key": "source_generation", "match": {"value": source_generation}},
                    {"key": "source_index_version", "match": {"value": source_index_version}}
                ]
            }
        });
        if let Some(next_offset) = offset.take() {
            body["offset"] = next_offset;
        }

        let response: Value = qdrant_post_json_with_retry(
            &client,
            &endpoint,
            &body,
            "qdrant_source_generation_point_counts",
            &cfg.collection,
            Instant::now(),
        )
        .await?;
        let page = response
            .pointer("/result/points")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("qdrant scroll response missing result.points"))?;
        points += page.len();
        for point in page {
            if let Some(item_key) = point
                .pointer("/payload/source_item_key")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                item_keys.insert(item_key.to_string());
            }
        }

        offset = response
            .pointer("/result/next_page_offset")
            .filter(|value| !value.is_null())
            .cloned();
        if offset.is_none() {
            break;
        }
    }

    Ok(SourceGenerationPointCounts {
        points,
        distinct_items: item_keys.len(),
    })
}

async fn qdrant_count_published_source_generation(
    cfg: &Config,
    source_id: &str,
    source_generation: i64,
    source_index_version: i64,
) -> Result<u64> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/count")?;
    let body = serde_json::json!({
        "exact": true,
        "filter": {
            "must": [
                {"key": "source_id", "match": {"value": source_id}},
                {"key": "source_generation", "match": {"value": source_generation}},
                {"key": "source_index_version", "match": {"value": source_index_version}},
                {"key": "source_committed", "match": {"value": true}}
            ]
        }
    });
    let response: Value = qdrant_post_json_with_retry(
        client,
        &endpoint,
        &body,
        "qdrant_count_published_source_generation",
        &cfg.collection,
        Instant::now(),
    )
    .await?;
    response
        .pointer("/result/count")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| anyhow!("qdrant count response missing result.count"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axon_core::config::Config;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn publish_source_generation_sets_visibility_and_verifies_count() -> Result<()> {
        let server = MockServer::start_async().await;
        let publish = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/payload")
                    .query_param("wait", "true")
                    .json_body_includes(r#"{"payload":{"source_committed":true}}"#);
                then.status(200)
                    .json_body(serde_json::json!({"result": {"status": "ok"}}));
            })
            .await;
        let count = server
            .mock_async(|when, then| {
                when.method(POST).path("/collections/test_col/points/count");
                then.status(200)
                    .json_body(serde_json::json!({"result": {"count": 3}}));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        qdrant_publish_source_generation(&cfg, "source-a", 7, 1, 3)
            .await
            .map_err(|err| anyhow!("publish failed: {err}"))?;

        publish.assert_async().await;
        count.assert_async().await;
        Ok(())
    }

    #[tokio::test]
    async fn publish_source_generation_fails_when_count_is_short() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/payload")
                    .query_param("wait", "true");
                then.status(200)
                    .json_body(serde_json::json!({"result": {"status": "ok"}}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/collections/test_col/points/count");
                then.status(200)
                    .json_body(serde_json::json!({"result": {"count": 2}}));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let err = qdrant_publish_source_generation(&cfg, "source-a", 7, 1, 3)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("expected at least 3"));
    }

    #[tokio::test]
    async fn source_generation_point_counts_scrolls_and_counts_distinct_items() -> Result<()> {
        let server = MockServer::start_async().await;
        let first = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/scroll")
                    .body_includes("source_generation")
                    .body_excludes("offset");
                then.status(200).json_body(serde_json::json!({
                    "result": {
                        "points": [
                            {"id": "1", "payload": {"source_item_key": "src/lib.rs"}},
                            {"id": "2", "payload": {"source_item_key": "src/lib.rs"}}
                        ],
                        "next_page_offset": "next"
                    }
                }));
            })
            .await;
        let second = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/scroll")
                    .body_includes("\"offset\":\"next\"");
                then.status(200).json_body(serde_json::json!({
                    "result": {
                        "points": [
                            {"id": "3", "payload": {"source_item_key": "README.md"}}
                        ],
                        "next_page_offset": null
                    }
                }));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let counts = qdrant_source_generation_point_counts(&cfg, "source-a", 7, 1)
            .await
            .map_err(|err| anyhow!("count failed: {err}"))?;

        assert_eq!(
            counts,
            SourceGenerationPointCounts {
                points: 3,
                distinct_items: 2,
            }
        );
        first.assert_async().await;
        second.assert_async().await;
        Ok(())
    }
}
