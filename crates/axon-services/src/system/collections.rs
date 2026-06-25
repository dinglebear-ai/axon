//! Qdrant collection discovery.

use crate::types::CollectionsResult;
use axon_core::config::Config;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum CollectionsError {
    #[error("failed to build qdrant metadata client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("qdrant collections request failed: {0}")]
    Request(reqwest::Error),
    #[error("qdrant collections request failed: {0}")]
    Status(reqwest::StatusCode),
    #[error("qdrant returned invalid collections response: {0}")]
    InvalidResponse(reqwest::Error),
}

#[must_use = "collections returns a Result that should be handled"]
pub async fn collections(cfg: &Config) -> Result<CollectionsResult, CollectionsError> {
    let url = format!("{}/collections", cfg.qdrant_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(CollectionsError::ClientBuild)?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(CollectionsError::Request)?;
    if !resp.status().is_success() {
        return Err(CollectionsError::Status(resp.status()));
    }

    let value = resp
        .json::<serde_json::Value>()
        .await
        .map_err(CollectionsError::InvalidResponse)?;
    Ok(map_collections_payload(&value))
}

pub fn map_collections_payload(value: &serde_json::Value) -> CollectionsResult {
    let mut collections = value
        .get("result")
        .and_then(|v| v.get("collections"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("name").and_then(|name| name.as_str()))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    collections.sort();
    CollectionsResult { collections }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::MockServer;

    fn cfg_for_qdrant(url: String) -> Config {
        Config {
            qdrant_url: url,
            ..Config::default()
        }
    }

    #[test]
    fn map_collections_payload_sorts_and_ignores_invalid_entries() {
        let payload = serde_json::json!({
            "result": {
                "collections": [
                    { "name": "zeta" },
                    { "name": 42 },
                    { "other": "ignored" },
                    { "name": "axon" }
                ]
            }
        });

        let result = map_collections_payload(&payload);

        assert_eq!(result.collections, vec!["axon", "zeta"]);
    }

    #[tokio::test]
    async fn collections_fetches_qdrant_names() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/collections");
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "collections": [
                        { "name": "docs" },
                        { "name": "axon" }
                    ]
                }
            }));
        });
        let cfg = cfg_for_qdrant(server.base_url());

        let result = collections(&cfg).await.expect("collections result");

        mock.assert();
        assert_eq!(result.collections, vec!["axon", "docs"]);
    }

    #[tokio::test]
    async fn collections_maps_non_success_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/collections");
            then.status(503).body("not today");
        });
        let cfg = cfg_for_qdrant(server.base_url());

        let err = collections(&cfg).await.expect_err("status error");

        assert!(matches!(
            err,
            CollectionsError::Status(reqwest::StatusCode::SERVICE_UNAVAILABLE)
        ));
    }

    #[tokio::test]
    async fn collections_maps_invalid_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/collections");
            then.status(200).body("not json");
        });
        let cfg = cfg_for_qdrant(server.base_url());

        let err = collections(&cfg).await.expect_err("invalid json error");

        assert!(matches!(err, CollectionsError::InvalidResponse(_)));
    }
}
