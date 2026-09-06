//! Credential-aware Qdrant control-plane operations.

use axon_api::source::{ApiError, ErrorStage};

use super::QdrantVectorStore;
use crate::store::Result;

impl QdrantVectorStore {
    pub async fn count_selector_points(
        &self,
        selector: &axon_api::source::VectorDeleteSelector,
        stage: ErrorStage,
    ) -> Result<u64> {
        crate::filter::validate_delete_selector(selector)?;
        let collection = crate::filter::selector_collection(selector);
        let http = self.http()?;
        self.require_collection_spec(&http, collection, stage)
            .await?;
        if matches!(
            selector,
            axon_api::source::VectorDeleteSelector::Collection { .. }
        ) {
            return super::store_impl::count_all_points(&http, collection, stage).await;
        }
        super::store_impl::count_delete_matches(
            &http,
            collection,
            &super::store_impl::delete_body(selector)?,
            stage,
        )
        .await
    }

    pub async fn service_ready(&self) -> Result<bool> {
        let http = self.http()?;
        let url = http.endpoint().service_path("readyz");
        Ok(http
            .get_json(ErrorStage::Observing, &url, "qdrant_ready")
            .await?
            .is_some())
    }

    pub async fn list_collections_json(&self) -> Result<serde_json::Value> {
        let http = self.http()?;
        let url = http.endpoint().service_path("collections");
        http.get_json(ErrorStage::Observing, &url, "qdrant_collections")
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "vector.qdrant.not_found",
                    ErrorStage::Observing,
                    "Qdrant collections endpoint was not found",
                )
            })
    }

    pub async fn collection_info_json(
        &self,
        collection: &str,
    ) -> Result<Option<serde_json::Value>> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "");
        http.get_json(ErrorStage::Observing, &url, "qdrant_collection_info")
            .await
    }

    pub async fn drop_collection(&self, collection: &str) -> Result<bool> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "");
        http.delete(ErrorStage::Cleaning, &url, "qdrant_drop_collection")
            .await
    }

    pub async fn create_collection_json(
        &self,
        collection: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "");
        http.put_json(
            ErrorStage::Upserting,
            &url,
            body,
            "qdrant_create_collection",
        )
        .await
    }

    pub async fn post_collection_json(
        &self,
        collection: &str,
        suffix: &str,
        body: &serde_json::Value,
        stage: ErrorStage,
    ) -> Result<serde_json::Value> {
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, suffix);
        http.post_json(stage, &url, body, "qdrant_admin").await
    }
}
