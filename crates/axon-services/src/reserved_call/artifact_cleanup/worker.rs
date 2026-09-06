use super::*;

pub(super) struct ArtifactCleanupFailure {
    pub(super) error: ApiError,
    pub(super) work: ArtifactCleanupWork,
}

pub(in crate::reserved_call) static ARTIFACT_CLEANUP_WORKERS: std::sync::LazyLock<
    axon_core::detached_workers::DetachedWorkerRegistry,
> = std::sync::LazyLock::new(axon_core::detached_workers::DetachedWorkerRegistry::default);
pub(in crate::reserved_call) static UNRESOLVED_ARTIFACT_CLEANUPS: std::sync::LazyLock<
    std::sync::Mutex<Vec<ArtifactCleanupWork>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(test)]
pub(crate) fn drain_artifact_cleanup_workers() -> usize {
    ARTIFACT_CLEANUP_WORKERS.drain();
    drain_unresolved_artifact_cleanups()
}

pub(super) async fn rollback_untracked_artifacts(
    work: &ArtifactCleanupWork,
    artifacts: &[ArtifactRef],
) -> Vec<String> {
    let mut errors = Vec::new();
    for artifact in artifacts {
        let debt = artifact_cleanup_debt(work, artifact);
        let debt_owned = match work.ledger.record_cleanup_debt(debt.clone()).await {
            Ok(()) => true,
            Err(error) => {
                errors.push(format!("debt {}: {error}", artifact.artifact_id.0));
                false
            }
        };
        let handle = ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            uri: None,
        };
        match delete_artifact_reserved(work, handle).await {
            Ok(()) if debt_owned => {
                if let Err(error) = work.ledger.resolve_cleanup_debt(debt.debt_id).await {
                    errors.push(format!("resolve {}: {error}", artifact.artifact_id.0));
                }
            }
            Ok(()) => {}
            Err(error) => errors.push(format!("delete {}: {error}", artifact.artifact_id.0)),
        }
    }
    errors
}

pub(super) async fn cleanup_artifacts(
    work: ArtifactCleanupWork,
) -> Result<(), ArtifactCleanupFailure> {
    cleanup_artifacts_with_progress(work, None).await
}

async fn cleanup_artifacts_with_progress(
    mut work: ArtifactCleanupWork,
    progress: Option<&(dyn Fn(&ArtifactCleanupWork) + Send + Sync)>,
) -> Result<(), ArtifactCleanupFailure> {
    match work
        .ledger
        .committed_generation(work.source_id.clone())
        .await
    {
        Ok(Some(committed)) if committed == work.generation => {
            if let Some(token) = &work.journal {
                if let Err(error) = artifact_cleanup_journal::remove(token).await {
                    return Err(ArtifactCleanupFailure {
                        error: ApiError::new(
                            "artifact.cleanup_journal_remove_failed",
                            ErrorStage::Cleaning,
                            error.to_string(),
                        ),
                        work,
                    });
                }
            }
            return Ok(());
        }
        Ok(_) => {}
        Err(error) => return Err(ArtifactCleanupFailure { error, work }),
    }
    let artifacts = std::mem::take(&mut work.artifacts);
    for (index, artifact) in artifacts.iter().enumerate() {
        let mut debt = artifact_cleanup_debt(&work, artifact);
        if let Err(error) = work.ledger.record_cleanup_debt(debt.clone()).await {
            work.artifacts.extend_from_slice(&artifacts[index..]);
            if let Some(token) = &work.journal {
                let _ = artifact_cleanup_journal::rewrite(token, &work).await;
            }
            return Err(ArtifactCleanupFailure { error, work });
        }
        let handle = ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            // Cleanup is identity based. Persisting or replaying the original URI would
            // unnecessarily retain a potentially sensitive provider location.
            uri: None,
        };
        if let Err(error) = delete_artifact_reserved(&work, handle).await {
            debt.attempts = 1;
            debt.last_error = Some(SourceError {
                code: error.code.0,
                severity: Severity::Warning,
                message: error.message,
                source_item_key: None,
                retryable: true,
                provider_id: error.provider_id.map(ProviderId::new),
                cause: None,
            });
            let _ = work.ledger.record_cleanup_debt(debt).await;
        } else if let Err(error) = work.ledger.resolve_cleanup_debt(debt.debt_id).await {
            tracing::warn!(artifact_id = %artifact.artifact_id.0, error = %error, "artifact deleted but cleanup debt resolution failed");
        }
        work.artifacts.extend_from_slice(&artifacts[index + 1..]);
        if let Some(token) = &work.journal
            && let Err(error) = artifact_cleanup_journal::rewrite(token, &work).await
        {
            return Err(ArtifactCleanupFailure {
                error: ApiError::new(
                    "artifact.cleanup_journal_failed",
                    ErrorStage::Cleaning,
                    error.to_string(),
                ),
                work,
            });
        }
        if let Some(progress) = progress {
            progress(&work);
        }
        work.artifacts.clear();
    }
    if let Some(token) = &work.journal
        && let Err(error) = artifact_cleanup_journal::remove(token).await
    {
        return Err(ArtifactCleanupFailure {
            error: ApiError::new(
                "artifact.cleanup_journal_remove_failed",
                ErrorStage::Cleaning,
                error.to_string(),
            ),
            work,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::reserved_call) enum CleanupWorkerFault {
    Spawn,
    RuntimeBuild,
    PanicAfterHandoff,
    DrainPanicAfterHandoff,
    PanicAfterFirstProgress,
    DrainPanicOnSecondEntry,
}

pub(in crate::reserved_call) fn spawn_artifact_cleanup_retry(work: ArtifactCleanupWork) {
    spawn_artifact_cleanup_retry_inner(work, None);
}

pub(in crate::reserved_call) fn spawn_artifact_cleanup_retry_inner(
    work: ArtifactCleanupWork,
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<CleanupWorkerFault>,
) {
    let handoff = Arc::new(std::sync::Mutex::new(Some(work)));
    #[cfg(test)]
    if fault == Some(CleanupWorkerFault::Spawn) {
        UNRESOLVED_ARTIFACT_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(
                handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .unwrap(),
            );
        return;
    }
    let worker_handoff = Arc::clone(&handoff);
    let worker = std::thread::Builder::new()
        .name("axon-artifact-cleanup".to_string())
        .spawn(move || run_retry_worker(worker_handoff, fault));
    match worker {
        Ok(worker) => ARTIFACT_CLEANUP_WORKERS.track(worker),
        Err(error) => {
            tracing::error!(%error, "failed to spawn artifact cleanup worker");
            if let Some(work) = handoff
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                UNRESOLVED_ARTIFACT_CLEANUPS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(work);
            }
        }
    }
}

fn run_retry_worker(
    worker_handoff: Arc<std::sync::Mutex<Option<ArtifactCleanupWork>>>,
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<CleanupWorkerFault>,
) {
    let panic_handoff = Arc::clone(&worker_handoff);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_retry_loop(&worker_handoff, fault)
    }));
    if outcome.is_err()
        && let Some(work) = panic_handoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    {
        UNRESOLVED_ARTIFACT_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(work);
    }
}

fn run_retry_loop(
    worker_handoff: &Arc<std::sync::Mutex<Option<ArtifactCleanupWork>>>,
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<CleanupWorkerFault>,
) {
    let mut pending = worker_handoff
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .cloned()
        .expect("artifact cleanup worker owns handoff");
    #[cfg(test)]
    if fault == Some(CleanupWorkerFault::PanicAfterHandoff) {
        panic!("injected artifact cleanup worker panic after handoff");
    }
    #[cfg(test)]
    if fault == Some(CleanupWorkerFault::RuntimeBuild) {
        retain_handoff(worker_handoff);
        return;
    }
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        tracing::error!("failed to build artifact cleanup retry runtime");
        retain_handoff(worker_handoff);
        return;
    };
    if pending.journal.is_none() {
        match runtime.block_on(artifact_cleanup_journal::persist(
            &artifact_cleanup_journal::default_root(),
            &pending,
        )) {
            Ok(token) => pending.journal = Some(token),
            Err(error) => tracing::error!(%error, "artifact cleanup retry journal is unavailable"),
        }
    }
    let progress_handoff = Arc::clone(worker_handoff);
    #[cfg(test)]
    let panicked_after_progress = std::sync::atomic::AtomicBool::new(false);
    let progress = |work: &ArtifactCleanupWork| {
        *progress_handoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(work.clone());
        #[cfg(test)]
        if fault == Some(CleanupWorkerFault::PanicAfterFirstProgress)
            && !panicked_after_progress.swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            panic!("injected artifact cleanup panic after durable progress");
        }
    };
    for retry in 0..3 {
        match runtime.block_on(cleanup_artifacts_with_progress(pending, Some(&progress))) {
            Ok(()) => {
                worker_handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                return;
            }
            Err(failure) => {
                tracing::error!(error = %failure.error, unresolved = unresolved_cleanup_units(&failure.work), "artifact cleanup handoff remains unresolved; retrying");
                pending = failure.work;
                *worker_handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pending.clone());
                if retry < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    retain_handoff(worker_handoff);
}

fn retain_handoff(worker_handoff: &Arc<std::sync::Mutex<Option<ArtifactCleanupWork>>>) {
    if let Some(work) = worker_handoff
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    {
        UNRESOLVED_ARTIFACT_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(work);
    }
}

pub(super) fn drain_unresolved_artifact_cleanups() -> usize {
    drain_unresolved_artifact_cleanups_inner(None)
}

pub(in crate::reserved_call) fn drain_unresolved_artifact_cleanups_inner(
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<CleanupWorkerFault>,
) -> usize {
    let unresolved = {
        let mut pending = UNRESOLVED_ARTIFACT_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *pending)
    };
    if unresolved.is_empty() {
        return 0;
    }
    let handoff = Arc::new(std::sync::Mutex::new(Some(unresolved)));
    let worker_handoff = Arc::clone(&handoff);
    let worker = std::thread::Builder::new()
        .name("axon-artifact-cleanup-drain".into())
        .spawn(move || {
            let unresolved = worker_handoff
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .cloned()
                .unwrap_or_default();
            #[cfg(test)]
            if fault == Some(CleanupWorkerFault::DrainPanicAfterHandoff) {
                panic!("injected artifact cleanup drain panic after handoff");
            }
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return unresolved;
            };
            let total = unresolved.len();
            for iteration in 0..total {
                #[cfg(not(test))]
                let _ = iteration;
                #[cfg(test)]
                if fault == Some(CleanupWorkerFault::DrainPanicOnSecondEntry) && iteration == 1 {
                    panic!("injected artifact cleanup drain panic on second entry");
                }
                let work = worker_handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .and_then(|pending| pending.first().cloned())
                    .expect("drain handoff owns current work");
                let progress_handoff = Arc::clone(&worker_handoff);
                let progress = move |work: &ArtifactCleanupWork| {
                    if let Some(first) = progress_handoff
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .as_mut()
                        .and_then(|pending| pending.first_mut())
                    {
                        *first = work.clone();
                    }
                };
                let result =
                    runtime.block_on(cleanup_artifacts_with_progress(work, Some(&progress)));
                let mut owned = worker_handoff
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let pending = owned.as_mut().expect("drain handoff remains owned");
                match result {
                    Ok(()) => {
                        pending.remove(0);
                    }
                    Err(failure) => {
                        pending[0] = failure.work;
                        pending.rotate_left(1);
                    }
                }
            }
            worker_handoff
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .unwrap_or_default()
        });
    let remaining = match worker {
        Ok(worker) => match worker.join() {
            Ok(remaining) => remaining,
            Err(_) => handoff
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .unwrap_or_default(),
        },
        Err(error) => {
            tracing::error!(%error, "failed to spawn artifact cleanup drain worker");
            handoff
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .unwrap_or_default()
        }
    };
    let count = remaining.iter().map(unresolved_cleanup_units).sum();
    UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(remaining);
    count
}

pub(in crate::reserved_call) fn unresolved_cleanup_units(work: &ArtifactCleanupWork) -> usize {
    work.artifacts
        .len()
        .max(usize::from(work.journal.is_some()))
}

fn artifact_cleanup_debt(work: &ArtifactCleanupWork, artifact: &ArtifactRef) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "artifact-cleanup:{}:{}:{}",
                    work.source_id.0, work.generation.0, artifact.artifact_id.0
                )
                .as_bytes(),
            )
        )),
        job_id: work.job_id,
        origin_attempt: work.attempt,
        source_id: work.source_id.clone(),
        generation: Some(work.generation.clone()),
        kind: CleanupDebtKind::ArtifactDelete,
        selector: CleanupSelector::Artifact {
            artifact_id: artifact.artifact_id.clone(),
        },
        vector_collection: None,
        status: LifecycleStatus::Pending,
        created_at: Timestamp::from(chrono::Utc::now()),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}
