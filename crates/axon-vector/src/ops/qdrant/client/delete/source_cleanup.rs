use anyhow::{Result, anyhow};
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use serde::Deserialize;

use super::super::super::utils::qdrant_collection_endpoint;
use super::qdrant_delete_with_retry;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CleanupSelectorV1 {
    pub collection: String,
    pub source_id: String,
    pub source_index_version: i64,
    pub source_generation: i64,
    #[serde(alias = "source_item_key")]
    pub item_key: String,
}

impl CleanupSelectorV1 {
    pub fn new(
        collection: impl Into<String>,
        source_id: impl Into<String>,
        source_index_version: i64,
        source_generation: i64,
        item_key: impl Into<String>,
    ) -> Result<Self> {
        let selector = Self {
            collection: collection.into(),
            source_id: source_id.into(),
            source_index_version,
            source_generation,
            item_key: item_key.into(),
        };
        if selector.collection.trim().is_empty() {
            return Err(anyhow!("cleanup selector collection cannot be empty"));
        }
        if selector.source_id.trim().is_empty() {
            return Err(anyhow!("cleanup selector source_id cannot be empty"));
        }
        if selector.item_key.trim().is_empty() {
            return Err(anyhow!("cleanup selector item_key cannot be empty"));
        }
        Ok(selector)
    }

    pub fn filter(&self) -> serde_json::Value {
        serde_json::json!({
            "must": [
                {"key": "source_id", "match": {"value": self.source_id}},
                {"key": "source_index_version", "match": {"value": self.source_index_version}},
                {"key": "source_generation", "match": {"value": self.source_generation}},
                {"key": "source_item_key", "match": {"value": self.item_key}}
            ]
        })
    }
}

pub async fn qdrant_delete_source_cleanup_selector(
    cfg: &Config,
    selector: &CleanupSelectorV1,
) -> Result<()> {
    if selector.collection != cfg.collection {
        return Err(anyhow!(
            "cleanup selector collection {} does not match active collection {}",
            selector.collection,
            cfg.collection
        ));
    }
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/delete?wait=true")?;
    qdrant_delete_with_retry(
        client,
        &endpoint,
        serde_json::json!({"filter": selector.filter()}),
        "qdrant_delete_source_cleanup_selector",
    )
    .await
}
