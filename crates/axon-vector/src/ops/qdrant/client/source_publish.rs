use anyhow::{Result, anyhow};
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use std::time::Instant;

use super::super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};

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
    let _: serde_json::Value = qdrant_post_json_with_retry(
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
    let response: serde_json::Value = qdrant_post_json_with_retry(
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
}
