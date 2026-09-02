//! Reference-counted Qdrant bulk-index lifecycle.

use std::time::{Duration, Instant};

use axon_api::source::{ApiError, ErrorStage};
use serde_json::json;

use super::{BULK_LOAD_USERS, QdrantVectorStore};
use crate::store::Result;

const OPTIMIZER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPTIMIZER_READY_TIMEOUT: Duration = Duration::from_secs(300);

impl QdrantVectorStore {
    pub(super) async fn begin_bulk_load_inner(&self, collection: &str) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let key = format!("{}\0{collection}", self.url.trim_end_matches('/'));
        let mut users = BULK_LOAD_USERS.lock().await;
        let count = users.entry(key.clone()).or_default();
        *count += 1;
        if *count > 1 {
            return Ok(());
        }
        if let Err(error) = self
            .set_indexing_threshold(collection, self.bulk_indexing_threshold)
            .await
        {
            users.remove(&key);
            return Err(error);
        }
        Ok(())
    }

    pub(super) async fn finish_bulk_load_inner(&self, collection: &str) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let key = format!("{}\0{collection}", self.url.trim_end_matches('/'));
        let mut users = BULK_LOAD_USERS.lock().await;
        let Some(count) = users.get_mut(&key) else {
            return Err(ApiError::new(
                "vector.qdrant.bulk_load_unbalanced",
                ErrorStage::Upserting,
                "bulk-load completion has no matching begin",
            ));
        };
        *count = count.saturating_sub(1);
        if *count > 0 {
            return Ok(());
        }
        users.remove(&key);
        self.set_indexing_threshold(collection, self.normal_indexing_threshold)
            .await?;
        self.wait_for_optimizer_ready(collection).await
    }

    async fn set_indexing_threshold(&self, collection: &str, threshold: u64) -> Result<()> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "");
        http.patch_json(
            ErrorStage::Upserting,
            &url,
            &json!({"optimizers_config": {"indexing_threshold": threshold}}),
            "qdrant_bulk_indexing_threshold",
        )
        .await
    }

    async fn wait_for_optimizer_ready(&self, collection: &str) -> Result<()> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "");
        let started = Instant::now();
        loop {
            let body = http
                .get_json(ErrorStage::Upserting, &url, "qdrant_optimizer_status")
                .await?
                .ok_or_else(|| {
                    ApiError::new(
                        "vector.qdrant.collection_missing",
                        ErrorStage::Upserting,
                        "collection disappeared while waiting for optimizer readiness",
                    )
                })?;
            let result = &body["result"];
            let status = result["status"].as_str().unwrap_or_default();
            let optimizer = result["optimizer_status"]
                .as_str()
                .or_else(|| result["optimizer_status"]["status"].as_str())
                .unwrap_or_default();
            if status == "green" && optimizer == "ok" {
                return Ok(());
            }
            if started.elapsed() >= OPTIMIZER_READY_TIMEOUT {
                return Err(ApiError::new(
                    "vector.qdrant.optimizer_timeout",
                    ErrorStage::Upserting,
                    "Qdrant optimizer did not become ready after restoring indexing",
                )
                .with_context("collection", collection.to_string()));
            }
            tokio::time::sleep(OPTIMIZER_POLL_INTERVAL).await;
        }
    }
}

#[cfg(test)]
#[path = "bulk_load_tests.rs"]
mod tests;
