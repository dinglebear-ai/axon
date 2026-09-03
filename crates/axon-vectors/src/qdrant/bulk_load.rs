//! Reference-counted Qdrant bulk-index lifecycle.

use std::time::{Duration, Instant};

use axon_api::source::{ApiError, ErrorStage};
use serde_json::json;

use super::{BULK_LOAD_USERS, QdrantVectorStore};
use crate::store::Result;

const OPTIMIZER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPTIMIZER_READY_TIMEOUT: Duration = Duration::from_secs(300);

enum CancelAction {
    RollBackBegin,
    CompleteFinish,
}

struct TransitionGuard {
    store: QdrantVectorStore,
    collection: String,
    key: String,
    entry: std::sync::Arc<tokio::sync::Mutex<usize>>,
    action: CancelAction,
    armed: bool,
}

impl TransitionGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TransitionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let store = self.store.clone();
        let collection = self.collection.clone();
        let key = self.key.clone();
        let entry = self.entry.clone();
        let roll_back = matches!(self.action, CancelAction::RollBackBegin);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let mut count = entry.lock().await;
                if roll_back {
                    *count = count.saturating_sub(1);
                }
                if *count == 0 {
                    let _ = store.restore_normal_indexing(&collection).await;
                    remove_idle_entry(&key, &entry, *count).await;
                }
            });
        }
    }
}

async fn remove_idle_entry(
    key: &str,
    entry: &std::sync::Arc<tokio::sync::Mutex<usize>>,
    count: usize,
) {
    if count != 0 {
        return;
    }
    let mut users = BULK_LOAD_USERS.lock().await;
    let removable = users
        .get(key)
        .is_some_and(|current| std::sync::Arc::ptr_eq(current, entry))
        && std::sync::Arc::strong_count(entry) == 2;
    if removable {
        users.remove(key);
    }
}

impl QdrantVectorStore {
    pub(super) async fn begin_bulk_load_inner(&self, collection: &str) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let key = format!("{}\0{collection}", self.url.trim_end_matches('/'));
        let entry = {
            let mut users = BULK_LOAD_USERS.lock().await;
            users
                .entry(key.clone())
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(0)))
                .clone()
        };
        let mut count = entry.lock().await;
        *count += 1;
        if *count > 1 {
            return Ok(());
        }
        let mut guard = TransitionGuard {
            store: self.clone(),
            collection: collection.to_string(),
            key: key.clone(),
            entry: entry.clone(),
            action: CancelAction::RollBackBegin,
            armed: true,
        };
        if let Err(mut error) = self
            .set_indexing_threshold(collection, self.bulk_indexing_threshold)
            .await
        {
            *count = count.saturating_sub(1);
            if let Err(compensation) = self.restore_normal_indexing(collection).await {
                error = error.with_context("compensation_error", compensation.to_string());
            }
            guard.disarm();
            drop(guard);
            remove_idle_entry(&key, &entry, *count).await;
            return Err(error);
        }
        guard.disarm();
        Ok(())
    }

    pub(super) async fn finish_bulk_load_inner(&self, collection: &str) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let key = format!("{}\0{collection}", self.url.trim_end_matches('/'));
        let entry = {
            let users = BULK_LOAD_USERS.lock().await;
            users.get(&key).cloned()
        };
        let Some(entry) = entry else {
            return Err(ApiError::new(
                "vector.qdrant.bulk_load_unbalanced",
                ErrorStage::Upserting,
                "bulk-load completion has no matching begin",
            ));
        };
        let mut count = entry.lock().await;
        *count = count.saturating_sub(1);
        if *count > 0 {
            return Ok(());
        }
        let mut guard = TransitionGuard {
            store: self.clone(),
            collection: collection.to_string(),
            key: key.clone(),
            entry: entry.clone(),
            action: CancelAction::CompleteFinish,
            armed: true,
        };
        let restoring = self.restore_normal_indexing(collection).await;
        guard.disarm();
        drop(guard);
        remove_idle_entry(&key, &entry, *count).await;
        restoring
    }

    async fn restore_normal_indexing(&self, collection: &str) -> Result<()> {
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
