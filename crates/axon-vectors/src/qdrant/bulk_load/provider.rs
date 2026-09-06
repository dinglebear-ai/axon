use std::time::Instant;

use axon_api::source::{ApiError, ErrorStage};
use serde_json::json;

use super::{OPTIMIZER_POLL_INTERVAL, OPTIMIZER_READY_TIMEOUT};
use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

impl QdrantVectorStore {
    pub(super) async fn restore_normal_indexing(&self, collection: &str) -> Result<()> {
        self.restore_indexing_threshold(collection, self.normal_indexing_threshold)
            .await
    }

    pub(super) async fn restore_indexing_threshold(
        &self,
        collection: &str,
        threshold: u64,
    ) -> Result<()> {
        self.set_indexing_threshold(collection, threshold).await?;
        self.wait_for_optimizer_ready(collection).await
    }

    pub(super) async fn set_indexing_threshold(
        &self,
        collection: &str,
        threshold: u64,
    ) -> Result<()> {
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

    pub(super) async fn wait_for_optimizer_ready(&self, collection: &str) -> Result<()> {
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
