//! Qdrant collection discovery.

use crate::core::config::Config;
use crate::services::types::CollectionsResult;
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
