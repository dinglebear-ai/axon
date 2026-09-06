use super::artifact_cleanup_journal;
use super::*;

mod worker;
#[cfg(test)]
pub(crate) use worker::drain_artifact_cleanup_workers;
pub(super) use worker::spawn_artifact_cleanup_retry;
#[cfg(test)]
pub(super) use worker::{
    ARTIFACT_CLEANUP_WORKERS, CleanupWorkerFault, UNRESOLVED_ARTIFACT_CLEANUPS,
    drain_unresolved_artifact_cleanups_inner, spawn_artifact_cleanup_retry_inner,
    unresolved_cleanup_units,
};
use worker::{cleanup_artifacts, rollback_untracked_artifacts};

/// Drains cancellation-triggered provider cleanup before a process runtime exits.
pub struct BulkLoadCleanupDrain;

impl Drop for BulkLoadCleanupDrain {
    fn drop(&mut self) {
        drain_bulk_load_cleanups();
        worker::ARTIFACT_CLEANUP_WORKERS.drain();
        let unresolved = worker::drain_unresolved_artifact_cleanups();
        if unresolved > 0 {
            tracing::error!(
                unresolved,
                "artifact cleanup remains unresolved after shutdown drain; durable journals will be replayed on restart"
            );
        }
    }
}
pub struct ArtifactCleanupGuard {
    store: Arc<dyn ArtifactStore>,
    ledger: Arc<dyn LedgerStore>,
    scheduler: Option<Arc<ProviderScheduler>>,
    job_id: JobId,
    attempt: u32,
    source_id: SourceId,
    generation: SourceGenerationId,
    artifacts: Vec<ArtifactRef>,
    journal: Option<artifact_cleanup_journal::JournalToken>,
    armed: bool,
}

impl ArtifactCleanupGuard {
    pub fn new(
        runtime: &TargetLocalSourceRuntime,
        job_id: JobId,
        attempt: u32,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Self {
        Self {
            store: Arc::clone(&runtime.artifact_store),
            ledger: Arc::clone(&runtime.ledger),
            scheduler: runtime.artifact_scheduler.clone(),
            job_id,
            attempt,
            source_id,
            generation,
            artifacts: Vec::new(),
            journal: None,
            armed: true,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(
        store: Arc<dyn ArtifactStore>,
        ledger: Arc<dyn LedgerStore>,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Self {
        Self {
            store,
            ledger,
            scheduler: None,
            job_id: JobId::new(uuid::Uuid::new_v4()),
            attempt: 0,
            source_id,
            generation,
            artifacts: Vec::new(),
            journal: None,
            armed: true,
        }
    }

    #[cfg(test)]
    pub fn new_for_test_with_scheduler(
        store: Arc<dyn ArtifactStore>,
        ledger: Arc<dyn LedgerStore>,
        scheduler: Arc<ProviderScheduler>,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Self {
        let mut guard = Self::new_for_test(store, ledger, source_id, generation);
        guard.job_id = JobId::new(uuid::Uuid::nil());
        guard.scheduler = Some(scheduler);
        guard
    }

    pub async fn track(&mut self, artifacts: &[ArtifactRef]) -> Result<(), ApiError> {
        let mut candidate = self.artifacts.clone();
        let mut newly_tracked = Vec::new();
        for artifact in artifacts {
            if candidate
                .iter()
                .all(|tracked| tracked.artifact_id != artifact.artifact_id)
            {
                candidate.push(artifact.clone());
                newly_tracked.push(artifact.clone());
            }
        }
        if candidate == self.artifacts {
            return Ok(());
        }
        let work = self.work(candidate.clone(), self.journal.clone());
        let result = match &self.journal {
            Some(token) => artifact_cleanup_journal::rewrite(token, &work)
                .await
                .map(|()| token.clone()),
            None => {
                artifact_cleanup_journal::persist(&artifact_cleanup_journal::default_root(), &work)
                    .await
            }
        };
        let token = match result {
            Ok(token) => token,
            Err(error) => {
                let rollback_errors = rollback_untracked_artifacts(&work, &newly_tracked).await;
                let message = if rollback_errors.is_empty() {
                    error.to_string()
                } else {
                    format!("{error}; artifact rollback: {}", rollback_errors.join("; "))
                };
                return Err(ApiError::new(
                    "artifact.cleanup_journal_failed",
                    ErrorStage::Cleaning,
                    message,
                ));
            }
        };
        self.artifacts = candidate;
        self.journal = Some(token);
        Ok(())
    }

    pub async fn disarm(&mut self) -> Result<(), ApiError> {
        // Publication owns these artifacts now. Make Drop non-destructive before
        // awaiting journal removal so cancellation cannot resurrect cleanup.
        self.armed = false;
        if let Some(token) = &self.journal {
            artifact_cleanup_journal::remove(token)
                .await
                .map_err(|error| {
                    ApiError::new(
                        "artifact.cleanup_journal_remove_failed",
                        ErrorStage::Cleaning,
                        error.to_string(),
                    )
                })?;
        }
        self.journal = None;
        self.artifacts.clear();
        Ok(())
    }

    #[cfg(test)]
    pub async fn disarm_then_panic_after_remove_for_test(&mut self) {
        self.armed = false;
        if let Some(token) = &self.journal {
            artifact_cleanup_journal::remove(token).await.unwrap();
        }
        panic!("injected cancellation after journal removal");
    }

    /// Complete compensating cleanup before the owning source lifecycle exits.
    pub async fn finish(&mut self) -> Result<(), ApiError> {
        let Some(work) = self.take_work() else {
            return Ok(());
        };
        match cleanup_artifacts(work).await {
            Ok(()) => Ok(()),
            Err(failure) => {
                let error = failure.error.clone();
                spawn_artifact_cleanup_retry(failure.work);
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub fn tracked_artifact_count_for_test(&self) -> usize {
        self.artifacts.len()
    }

    #[cfg(test)]
    pub fn inject_next_track_persist_failure_for_test(&self, artifacts: Vec<ArtifactRef>) {
        artifact_cleanup_journal::inject_next_persist_failure(&self.work(artifacts, None));
    }

    fn take_work(&mut self) -> Option<ArtifactCleanupWork> {
        if !self.armed || self.artifacts.is_empty() {
            self.armed = false;
            return None;
        }
        self.armed = false;
        Some(ArtifactCleanupWork {
            store: Arc::clone(&self.store),
            ledger: Arc::clone(&self.ledger),
            scheduler: self.scheduler.clone(),
            job_id: self.job_id,
            attempt: self.attempt,
            source_id: self.source_id.clone(),
            generation: self.generation.clone(),
            artifacts: std::mem::take(&mut self.artifacts),
            journal: self.journal.take(),
        })
    }

    fn work(
        &self,
        artifacts: Vec<ArtifactRef>,
        journal: Option<artifact_cleanup_journal::JournalToken>,
    ) -> ArtifactCleanupWork {
        ArtifactCleanupWork {
            store: Arc::clone(&self.store),
            ledger: Arc::clone(&self.ledger),
            scheduler: self.scheduler.clone(),
            job_id: self.job_id,
            attempt: self.attempt,
            source_id: self.source_id.clone(),
            generation: self.generation.clone(),
            artifacts,
            journal,
        }
    }
}

#[derive(Clone)]
pub(super) struct ArtifactCleanupWork {
    pub(super) store: Arc<dyn ArtifactStore>,
    pub(super) ledger: Arc<dyn LedgerStore>,
    pub(super) scheduler: Option<Arc<ProviderScheduler>>,
    pub(super) job_id: JobId,
    pub(super) attempt: u32,
    pub(super) source_id: SourceId,
    pub(super) generation: SourceGenerationId,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) journal: Option<artifact_cleanup_journal::JournalToken>,
}

async fn delete_artifact_reserved(
    work: &ArtifactCleanupWork,
    handle: ArtifactHandle,
) -> Result<(), ApiError> {
    let Some(scheduler) = work.scheduler.as_deref() else {
        return work.store.delete(handle).await;
    };
    let store = Arc::clone(&work.store);
    let context = ProviderCallContext::for_phase(
        work.job_id,
        work.attempt,
        PipelinePhase::Cleaning,
        JobPriority::Background,
        format!("artifact-cleanup:{}", uuid::Uuid::new_v4()),
    );
    map_reserved(
        call_reserved::<ArtifactLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.delete(handle).await },
        )
        .await,
        ErrorStage::Cleaning,
        "artifact",
    )
}

impl Drop for ArtifactCleanupGuard {
    fn drop(&mut self) {
        let Some(work) = self.take_work() else {
            return;
        };
        if work.journal.is_none() {
            tracing::error!(
                "artifact cleanup invariant violated: refusing memory-only detached handoff"
            );
            return;
        }
        spawn_artifact_cleanup_retry(work);
    }
}
