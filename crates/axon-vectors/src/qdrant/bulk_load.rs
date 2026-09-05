//! Reference-counted Qdrant bulk-index lifecycle.

use std::path::PathBuf;
use std::time::Duration;

use axon_api::source::{ApiError, ErrorStage};
use axon_core::detached_workers::DetachedWorkerRegistry;

use super::QdrantVectorStore;
use crate::store::Result;

const OPTIMIZER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPTIMIZER_READY_TIMEOUT: Duration = Duration::from_secs(300);

mod journal;
mod provider;
use journal::BulkLoadJournal;
#[cfg(test)]
use journal::{JournalWriteBoundary, PendingBulkLoad};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct BulkLoadKey {
    endpoint: String,
    collection: String,
}

#[derive(Debug)]
struct BulkLoadState {
    // LEARNED: aliases can join with different runtime knobs, so the last caller's
    // configuration is not necessarily the baseline changed by the first owner.
    // PATTERN: capture restoration state at first admission and retain it until drain.
    users: usize,
    restore_threshold: u64,
    // Held from the first begin through the final finish. The OS releases it
    // on crash, serializing bulk-index mode across independent Axon processes.
    process_lease: Option<std::fs::File>,
}

static BULK_LOAD_USERS: std::sync::LazyLock<
    tokio::sync::Mutex<
        std::collections::HashMap<BulkLoadKey, std::sync::Arc<tokio::sync::Mutex<BulkLoadState>>>,
    >,
> = std::sync::LazyLock::new(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));

fn configured_journal() -> Result<Option<BulkLoadJournal>> {
    #[cfg(test)]
    let data_dir = Some(
        std::env::var_os("AXON_TEST_BULK_JOURNAL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("axon-vector-bulk-load-tests")),
    );
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

#[cfg(test)]
#[derive(Clone, Copy)]
enum TransitionWorkerFault {
    Spawn,
    Runtime,
}

fn track_transition_worker(worker: std::thread::JoinHandle<()>) {
    TRANSITION_WORKERS.track(worker);
}

/// Wait for every detached transition worker before process shutdown.
pub fn drain_bulk_load_transition_workers() {
    TRANSITION_WORKERS.drain();
}

async fn remove_idle_entry(
    key: &BulkLoadKey,
    entry: &std::sync::Arc<tokio::sync::Mutex<BulkLoadState>>,
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

    fn bulk_load_key(&self, collection: &str) -> Result<BulkLoadKey> {
        // LEARNED: raw endpoint spellings can differ only by credentials or query
        // parameters while still controlling the same Qdrant optimizer.
        // PATTERN: use the parsed redacted endpoint for both admission and recovery.
        Ok(BulkLoadKey {
            endpoint: self.bulk_journal_endpoint()?,
            collection: collection.to_string(),
        })
    }

    pub(super) async fn begin_bulk_load_inner(&self, collection: &str) -> Result<()> {
        self.begin_bulk_load_inner_with_fault(collection, None)
            .await
    }

    async fn begin_bulk_load_inner_with_fault(
        &self,
        collection: &str,
        #[cfg(test)] fault: Option<TransitionWorkerFault>,
        #[cfg(not(test))] _fault: Option<()>,
    ) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let store = self.clone();
        let collection = collection.to_string();
        let (completed, receiver) = tokio::sync::oneshot::channel();
        #[cfg(test)]
        if matches!(fault, Some(TransitionWorkerFault::Spawn)) {
            return Err(ApiError::new(
                "vector.qdrant.bulk_begin_spawn",
                ErrorStage::Upserting,
                "injected Qdrant bulk begin worker spawn failure",
            ));
        }
        let worker = std::thread::Builder::new()
            .name("qdrant-bulk-begin".into())
            .spawn(move || {
                #[cfg(test)]
                if matches!(fault, Some(TransitionWorkerFault::Runtime)) {
                    let _ = completed.send(Err(ApiError::new(
                        "vector.qdrant.bulk_begin_runtime",
                        ErrorStage::Upserting,
                        "injected Qdrant bulk begin runtime failure",
                    )));
                    return;
                }
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = completed.send(Err(ApiError::new(
                            "vector.qdrant.bulk_begin_runtime",
                            ErrorStage::Upserting,
                            format!("failed to build Qdrant bulk begin runtime: {error}"),
                        )));
                        return;
                    }
                };
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
        }).map_err(|error| ApiError::new(
            "vector.qdrant.bulk_begin_spawn",
            ErrorStage::Upserting,
            format!("failed to spawn Qdrant bulk begin worker: {error}"),
        ))?;
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
        let key = self.bulk_load_key(collection)?;
        let entry = {
            let mut users = BULK_LOAD_USERS.lock().await;
            users
                .entry(key.clone())
                .or_insert_with(|| {
                    std::sync::Arc::new(tokio::sync::Mutex::new(BulkLoadState {
                        users: 0,
                        restore_threshold: self.normal_indexing_threshold,
                        process_lease: None,
                    }))
                })
                .clone()
        };
        let mut state = entry.lock().await;
        if state.users == 0 {
            if state.process_lease.is_some() {
                return Err(ApiError::new(
                    "vector.qdrant.bulk_recovery_required",
                    ErrorStage::Upserting,
                    "bulk-load restoration is still pending; recover it before starting a new bulk load",
                ));
            }
            let durable = journal.ok_or_else(|| {
                ApiError::new(
                    "vector.qdrant.bulk_lease_unavailable",
                    ErrorStage::Upserting,
                    "bulk loading requires AXON_DATA_DIR or HOME for cross-process ownership",
                )
            })?;
            state.process_lease =
                Some(durable.acquire_collection_lease(&key).map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.bulk_lease",
                        ErrorStage::Upserting,
                        format!("failed to acquire cross-process bulk-load lease: {error}"),
                    )
                })?);
        }
        state.users += 1;
        if state.users > 1 {
            return Ok(());
        }
        let journal_setup = if let Some(journal) = journal {
            journal
                .record(&key, self.normal_indexing_threshold)
                .map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.bulk_journal",
                        ErrorStage::Upserting,
                        format!("failed to persist bulk-load recovery state: {error}"),
                    )
                })
        } else {
            Ok(())
        };
        if let Err(error) = journal_setup {
            state.users = state.users.saturating_sub(1);
            state.process_lease = None;
            remove_idle_entry(&key, &entry, state.users).await;
            return Err(error);
        }
        if let Err(mut error) = self
            .set_indexing_threshold(collection, self.bulk_indexing_threshold)
            .await
        {
            state.users = state.users.saturating_sub(1);
            let compensated = match self.restore_normal_indexing(collection).await {
                Ok(()) => true,
                Err(compensation) => {
                    error = error.with_context("compensation_error", compensation.to_string());
                    false
                }
            };
            remove_idle_entry(&key, &entry, state.users).await;
            // LEARNED: an ambiguous provider failure plus failed compensation
            // is exactly the state for which the durable recovery record exists.
            // PATTERN: clear recovery intent only after compensation is known
            // to have restored the normal threshold, and preserve cleanup failure
            // as context without replacing the provider's primary error.
            if compensated
                && let Some(journal) = journal
                && let Err(cleanup) = journal.complete(&key)
            {
                tracing::error!(%cleanup, collection, "failed to clear compensated bulk-load recovery state");
                error = error.with_context("journal_cleanup_error", cleanup.to_string());
            }
            state.process_lease = None;
            return Err(error);
        }
        Ok(())
    }

    pub(super) async fn finish_bulk_load_inner(&self, collection: &str) -> Result<()> {
        self.finish_bulk_load_inner_with_fault(collection, None)
            .await
    }

    async fn finish_bulk_load_inner_with_fault(
        &self,
        collection: &str,
        #[cfg(test)] fault: Option<TransitionWorkerFault>,
        #[cfg(not(test))] _fault: Option<()>,
    ) -> Result<()> {
        if !self.bulk_load_enabled {
            return Ok(());
        }
        let store = self.clone();
        let collection = collection.to_string();
        let (completed, receiver) = tokio::sync::oneshot::channel();
        #[cfg(test)]
        if matches!(fault, Some(TransitionWorkerFault::Spawn)) {
            return Err(ApiError::new(
                "vector.qdrant.bulk_finish_spawn",
                ErrorStage::Upserting,
                "injected Qdrant bulk finish worker spawn failure",
            ));
        }
        let worker = std::thread::Builder::new()
            .name("qdrant-bulk-finish".into())
            .spawn(move || {
                #[cfg(test)]
                if matches!(fault, Some(TransitionWorkerFault::Runtime)) {
                    let _ = completed.send(Err(ApiError::new(
                        "vector.qdrant.bulk_finish_runtime",
                        ErrorStage::Upserting,
                        "injected Qdrant bulk finish runtime failure",
                    )));
                    return;
                }
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = completed.send(Err(ApiError::new(
                            "vector.qdrant.bulk_finish_runtime",
                            ErrorStage::Upserting,
                            format!("failed to build Qdrant bulk finish runtime: {error}"),
                        )));
                        return;
                    }
                };
            let result = runtime.block_on(store.finish_bulk_load_transition(&collection));
            if completed.send(result).is_err() {
                tracing::warn!(%collection, "bulk-load finish completed after caller cancellation");
            }
        }).map_err(|error| ApiError::new(
            "vector.qdrant.bulk_finish_spawn",
            ErrorStage::Upserting,
            format!("failed to spawn Qdrant bulk finish worker: {error}"),
        ))?;
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
        let journal = configured_journal();
        match journal {
            Ok(journal) => {
                self.finish_bulk_load_transition_with_journal(collection, Ok(journal.as_ref()))
                    .await
            }
            Err(error) => {
                self.finish_bulk_load_transition_with_journal(collection, Err(error))
                    .await
            }
        }
    }

    async fn finish_bulk_load_transition_with_journal(
        &self,
        collection: &str,
        journal: Result<Option<&BulkLoadJournal>>,
    ) -> Result<()> {
        let key = self.bulk_load_key(collection)?;
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
        let mut state = entry.lock().await;
        state.users = state.users.saturating_sub(1);
        if state.users > 0 {
            return Ok(());
        }
        let restoring = self
            .restore_indexing_threshold(collection, state.restore_threshold)
            .await;
        let journal_completion = if restoring.is_ok() {
            journal.and_then(|journal| match journal {
                Some(journal) => journal.complete(&key).map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.bulk_journal",
                        ErrorStage::Upserting,
                        format!("failed to clear bulk-load recovery state: {error}"),
                    )
                }),
                None => Ok(()),
            })
        } else {
            Ok(())
        };
        // A failed provider restore leaves the original recovery threshold live
        // in the journal. Retain both the registry entry and process lease so a
        // new begin cannot overwrite that baseline before recovery succeeds.
        // Once provider restoration succeeds, process-local ownership can be
        // evicted even if durable journal cleanup itself reports an error.
        if restoring.is_ok() {
            remove_idle_entry(&key, &entry, state.users).await;
            state.process_lease = None;
        }
        restoring.and(journal_completion)
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
            let key = BulkLoadKey {
                endpoint: endpoint.clone(),
                collection: transition.collection.clone(),
            };
            // A live owner retains this lease through restoration. Recovery
            // waits for it (or for the OS to release it after a crash) before
            // touching provider configuration.
            let _lease = journal.acquire_collection_lease(&key).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_lease",
                    ErrorStage::Upserting,
                    format!("failed to acquire recovery lease: {error}"),
                )
            })?;
            self.set_indexing_threshold(&transition.collection, transition.restore_threshold)
                .await?;
            self.wait_for_optimizer_ready(&transition.collection)
                .await?;
            journal.complete(&key).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_journal",
                    ErrorStage::Upserting,
                    format!("failed to clear recovered bulk-load state: {error}"),
                )
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "bulk_load_tests.rs"]
mod tests;
