use anyhow::{Result, anyhow};
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use serde::{Deserialize, Serialize};

use super::super::super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};
use super::qdrant_delete_with_retry;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CleanupSelectorV1 {
    collection: String,
    source_id: String,
    source_index_version: i64,
    source_generation: i64,
    #[serde(alias = "source_item_key")]
    item_key: String,
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
                {"key": "source_item_key", "match": {"any": [self.item_key.clone()]}}
            ]
        })
    }

    pub fn collection(&self) -> &str {
        &self.collection
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn source_index_version(&self) -> i64 {
        self.source_index_version
    }

    pub fn source_generation(&self) -> i64 {
        self.source_generation
    }

    pub fn item_key(&self) -> &str {
        &self.item_key
    }
}

pub(super) fn batch_filter(selectors: &[CleanupSelectorV1]) -> Result<serde_json::Value> {
    let first = selectors
        .first()
        .ok_or_else(|| anyhow!("cleanup selector batch cannot be empty"))?;
    let mut item_keys = Vec::with_capacity(selectors.len());
    for selector in selectors {
        if selector.collection != first.collection
            || selector.source_id != first.source_id
            || selector.source_index_version != first.source_index_version
            || selector.source_generation != first.source_generation
        {
            return Err(anyhow!(
                "cleanup selector batch must share collection, source_id, generation, and index_version"
            ));
        }
        if selector.item_key.trim().is_empty() {
            return Err(anyhow!("cleanup selector item_key cannot be empty"));
        }
        item_keys.push(serde_json::Value::String(selector.item_key.clone()));
    }
    Ok(serde_json::json!({
        "must": [
            {"key": "source_id", "match": {"value": first.source_id}},
            {"key": "source_index_version", "match": {"value": first.source_index_version}},
            {"key": "source_generation", "match": {"value": first.source_generation}},
            {"key": "source_item_key", "match": {"any": item_keys}}
        ]
    }))
}

pub async fn qdrant_delete_source_cleanup_selector(
    cfg: &Config,
    selector: &CleanupSelectorV1,
) -> Result<()> {
    qdrant_delete_source_cleanup_selectors(cfg, std::slice::from_ref(selector)).await
}

pub async fn qdrant_delete_source_cleanup_selectors(
    cfg: &Config,
    selectors: &[CleanupSelectorV1],
) -> Result<()> {
    let first = selectors
        .first()
        .ok_or_else(|| anyhow!("cleanup selector batch cannot be empty"))?;
    if first.collection != cfg.collection {
        return Err(anyhow!(
            "cleanup selector collection {} does not match active collection {}",
            first.collection,
            cfg.collection
        ));
    }
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/delete?wait=true")?;
    qdrant_delete_with_retry(
        client,
        &endpoint,
        serde_json::json!({"filter": batch_filter(selectors)?}),
        "qdrant_delete_source_cleanup_selectors",
    )
    .await?;
    let remaining = qdrant_count_source_cleanup_selectors(cfg, selectors).await?;
    if remaining != 0 {
        return Err(anyhow!(
            "source cleanup delete left {remaining} matching stale points"
        ));
    }
    Ok(())
}

async fn qdrant_count_source_cleanup_selectors(
    cfg: &Config,
    selectors: &[CleanupSelectorV1],
) -> Result<u64> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/count")?;
    let body = serde_json::json!({
        "exact": true,
        "filter": batch_filter(selectors)?
    });
    let response: serde_json::Value = qdrant_post_json_with_retry(
        client,
        &endpoint,
        &body,
        "qdrant_count_source_cleanup_selectors",
        &cfg.collection,
        Instant::now(),
    )
    .await?;
    response
        .pointer("/result/count")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| anyhow!("qdrant count response missing result.count"))
}
