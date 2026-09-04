//! Reference-counted Qdrant bulk-index lifecycle.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axon_api::source::{ApiError, ErrorStage};
use axon_core::detached_workers::DetachedWorkerRegistry;
use serde_json::json;

use super::{BULK_LOAD_USERS, QdrantVectorStore};
use crate::store::Result;

const OPTIMIZER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPTIMIZER_READY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PendingBulkLoad {
    endpoint: String,
    collection: String,
    restore_threshold: u64,
}

struct BulkLoadJournal {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalWriteBoundary {
    BeforeRename,
    BeforeParentSync,
}

static JOURNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl BulkLoadJournal {
    fn open(data_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            path: data_dir.join("qdrant-bulk-load-transitions.json"),
        })
    }

    fn pending(&self) -> std::io::Result<Vec<PendingBulkLoad>> {
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.read_unlocked()
    }

    fn record(
        &self,
        endpoint: &str,
        collection: &str,
        restore_threshold: u64,
    ) -> std::io::Result<()> {
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut pending = self.read_unlocked()?;
        pending.retain(|entry| entry.endpoint != endpoint || entry.collection != collection);
        pending.push(PendingBulkLoad {
            endpoint: endpoint.to_string(),
            collection: collection.to_string(),
            restore_threshold,
        });
        self.write_unlocked(&pending)
    }

    fn complete(&self, endpoint: &str, collection: &str) -> std::io::Result<()> {
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut pending = self.read_unlocked()?;
        pending.retain(|entry| entry.endpoint != endpoint || entry.collection != collection);
        self.write_unlocked(&pending)
    }

    fn read_unlocked(&self) -> std::io::Result<Vec<PendingBulkLoad>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn write_unlocked(&self, pending: &[PendingBulkLoad]) -> std::io::Result<()> {
        self.write_unlocked_with(pending, |_| Ok(()))
    }

    fn write_unlocked_with(
        &self,
        pending: &[PendingBulkLoad],
        mut at_boundary: impl FnMut(JournalWriteBoundary) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let result =
            (|| {
                let bytes = serde_json::to_vec(pending)?;
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                at_boundary(JournalWriteBoundary::BeforeRename)?;
                std::fs::rename(&temporary, &self.path)?;
                at_boundary(JournalWriteBoundary::BeforeParentSync)?;
                File::open(self.path.parent().ok_or_else(|| {
                    std::io::Error::other("bulk journal has no parent directory")
                })?)?
                .sync_all()
            })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

fn configured_journal() -> Result<Option<BulkLoadJournal>> {
    #[cfg(test)]
    let data_dir = std::env::var_os("AXON_TEST_BULK_JOURNAL_DIR").map(PathBuf::from);
    #[cfg(not(test))]
    let data_dir = std::env::var_os("AXON_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".axon")));
    data_dir
        .map(|path| {
            BulkLoadJournal::open(&path).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_journal",
                    ErrorStage::Upserting,
                    format!("failed to open bulk-load recovery journal: {error}"),
                )
            })
        })
        .transpose()
}

static TRANSITION_WORKERS: std::sync::LazyLock<DetachedWorkerRegistry> =
    std::sync::LazyLock::new(DetachedWorkerRegistry::default);

fn track_transition_worker(worker: std::thread::JoinHandle<()>) {
    TRANSITION_WORKERS.track(worker);
}

/// Wait for every detached transition worker before process shutdown.
pub fn drain_bulk_load_transition_workers() {
    TRANSITION_WORKERS.drain();
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
    fn bulk_journal_endpoint(&self) -> Result<String> {
        Ok(self
            .http()?
            .endpoint()
            .root()
            .trim_end_matches('/')
            .to_string())
    }

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
        let journal = configured_journal();
        match journal {
            Ok(journal) => {
                self.begin_bulk_load_transition_with_journal(collection, Ok(journal.as_ref()))
                    .await
            }
            Err(error) => {
                self.begin_bulk_load_transition_with_journal(collection, Err(error))
                    .await
            }
        }
    }

    async fn begin_bulk_load_transition_with_journal(
        &self,
        collection: &str,
        journal: Result<Option<&BulkLoadJournal>>,
    ) -> Result<()> {
        // LEARNED: reference counts are mutation state, so every fallible setup
        // exit after admission must release the owner it just registered.
        // PATTERN: resolve injected/configuration errors before admission, then
        // explicitly roll back admission if first-owner journal setup fails.
        let journal = journal?;
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
        let journal_setup = if let Some(journal) = journal {
            self.bulk_journal_endpoint().and_then(|endpoint| {
                journal
                    .record(&endpoint, collection, self.normal_indexing_threshold)
                    .map_err(|error| {
                        ApiError::new(
                            "vector.qdrant.bulk_journal",
                            ErrorStage::Upserting,
                            format!("failed to persist bulk-load recovery state: {error}"),
                        )
                    })
            })
        } else {
            Ok(())
        };
        if let Err(error) = journal_setup {
            *count = count.saturating_sub(1);
            remove_idle_entry(&key, &entry, *count).await;
            return Err(error);
        }
        if let Err(mut error) = self
            .set_indexing_threshold(collection, self.bulk_indexing_threshold)
            .await
        {
            *count = count.saturating_sub(1);
            let compensated = match self.restore_normal_indexing(collection).await {
                Ok(()) => true,
                Err(compensation) => {
                    error = error.with_context("compensation_error", compensation.to_string());
                    false
                }
            };
            remove_idle_entry(&key, &entry, *count).await;
            // LEARNED: an ambiguous provider failure plus failed compensation
            // is exactly the state for which the durable recovery record exists.
            // PATTERN: clear recovery intent only after compensation is known
            // to have restored the normal threshold.
            if compensated
                && let Some(journal) = journal
                && let Ok(endpoint) = self.bulk_journal_endpoint()
            {
                let _ = journal.complete(&endpoint, collection);
            }
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
        if restoring.is_ok()
            && let Some(journal) = configured_journal()?
        {
            let endpoint = self.bulk_journal_endpoint()?;
            journal.complete(&endpoint, collection).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_journal",
                    ErrorStage::Upserting,
                    format!("failed to clear bulk-load recovery state: {error}"),
                )
            })?;
        }
        remove_idle_entry(&key, &entry, *count).await;
        restoring
    }

    pub(super) async fn recover_bulk_load_transitions(&self) -> Result<()> {
        let Some(journal) = configured_journal()? else {
            return Ok(());
        };
        self.recover_bulk_load_transitions_from(&journal).await
    }

    async fn recover_bulk_load_transitions_from(&self, journal: &BulkLoadJournal) -> Result<()> {
        let endpoint = self.bulk_journal_endpoint()?;
        for transition in journal.pending().map_err(|error| {
            ApiError::new(
                "vector.qdrant.bulk_journal",
                ErrorStage::Upserting,
                format!("failed to read bulk-load recovery state: {error}"),
            )
        })? {
            if transition.endpoint != endpoint {
                continue;
            }
            self.set_indexing_threshold(&transition.collection, transition.restore_threshold)
                .await?;
            self.wait_for_optimizer_ready(&transition.collection)
                .await?;
            journal
                .complete(&endpoint, &transition.collection)
                .map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.bulk_journal",
                        ErrorStage::Upserting,
                        format!("failed to clear recovered bulk-load state: {error}"),
                    )
                })?;
        }
        Ok(())
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
