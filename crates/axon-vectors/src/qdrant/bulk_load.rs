//! Reference-counted Qdrant bulk-index lifecycle.

use std::time::{Duration, Instant};

use axon_api::source::{ApiError, ErrorStage};
use serde_json::json;

use super::{BULK_LOAD_USERS, QdrantVectorStore};
use crate::store::Result;

const OPTIMIZER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPTIMIZER_READY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Default)]
struct TransitionWorkers {
    handles: Vec<std::thread::JoinHandle<()>>,
    draining: bool,
}

static TRANSITION_WORKERS: std::sync::LazyLock<std::sync::Mutex<TransitionWorkers>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(TransitionWorkers::default()));

fn join_transition_worker(worker: std::thread::JoinHandle<()>) {
    if worker.join().is_err() {
        tracing::error!("Qdrant bulk-load transition worker panicked");
    }
}

fn track_transition_worker(worker: std::thread::JoinHandle<()>) {
    track_transition_worker_in(&TRANSITION_WORKERS, worker);
}

fn track_transition_worker_in(
    workers: &std::sync::Mutex<TransitionWorkers>,
    worker: std::thread::JoinHandle<()>,
) {
    let (finished, late_worker) = {
        let mut registry = workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut finished = Vec::new();
        let mut index = registry.handles.len();
        while index > 0 {
            index -= 1;
            if registry.handles[index].is_finished() {
                finished.push(registry.handles.swap_remove(index));
            }
        }
        if registry.draining {
            (finished, Some(worker))
        } else {
            registry.handles.push(worker);
            (finished, None)
        }
    };
    for worker in finished.into_iter().chain(late_worker) {
        join_transition_worker(worker);
    }
}

/// Wait for every detached transition worker before process shutdown.
pub fn drain_bulk_load_transition_workers() {
    drain_bulk_load_transition_workers_in(&TRANSITION_WORKERS);
}

fn drain_bulk_load_transition_workers_in(workers: &std::sync::Mutex<TransitionWorkers>) {
    let workers = {
        let mut registry = workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.draining = true;
        std::mem::take(&mut registry.handles)
    };
    for worker in workers {
        join_transition_worker(worker);
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
        let store = self.clone();
        let collection = collection.to_string();
        let (completed, receiver) = tokio::sync::oneshot::channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build Qdrant bulk begin runtime");
            let result = runtime.block_on(store.begin_bulk_load_transition(&collection));
            if let Err(result) = completed.send(result) {
                tracing::warn!(%collection, "bulk-load begin completed after caller cancellation");
                if result.is_ok()
                    && let Err(error) =
                        runtime.block_on(store.finish_bulk_load_transition(&collection))
                {
                    tracing::error!(%error, %collection, "failed to compensate bulk-load begin after caller cancellation");
                }
            }
        });
        track_transition_worker(worker);
        receiver.await.map_err(|_| {
            ApiError::new(
                "vector.qdrant.bulk_begin_join",
                ErrorStage::Upserting,
                "bulk-load begin worker stopped unexpectedly",
            )
        })?
    }

    async fn begin_bulk_load_transition(&self, collection: &str) -> Result<()> {
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
        if let Err(mut error) = self
            .set_indexing_threshold(collection, self.bulk_indexing_threshold)
            .await
        {
            *count = count.saturating_sub(1);
            if let Err(compensation) = self.restore_normal_indexing(collection).await {
                error = error.with_context("compensation_error", compensation.to_string());
            }
            remove_idle_entry(&key, &entry, *count).await;
            return Err(error);
        }
        Ok(())
    }

    pub(super) async fn finish_bulk_load_inner(&self, collection: &str) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let store = self.clone();
        let collection = collection.to_string();
        let (completed, receiver) = tokio::sync::oneshot::channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build Qdrant bulk finish runtime");
            let result = runtime.block_on(store.finish_bulk_load_transition(&collection));
            if completed.send(result).is_err() {
                tracing::warn!(%collection, "bulk-load finish completed after caller cancellation");
            }
        });
        track_transition_worker(worker);
        receiver.await.map_err(|_| {
            ApiError::new(
                "vector.qdrant.bulk_finish_join",
                ErrorStage::Upserting,
                "bulk-load finish worker stopped unexpectedly",
            )
        })?
    }

    async fn finish_bulk_load_transition(&self, collection: &str) -> Result<()> {
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
        let restoring = self.restore_normal_indexing(collection).await;
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
