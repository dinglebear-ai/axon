use std::{future::Future, sync::Arc, time::Duration};

use super::*;
use axon_api::source::LeaseGuard;
use axon_ledger::store::LedgerStore;
use tokio_util::task::AbortOnDropHandle;

pub(super) async fn run_with_lease<'a, F, Fut>(
    runtime: &'a TargetLocalSourceRuntime,
    input: &mut SourcePipelineInput<'a>,
    emitter: &'a SourceEventEmitter,
    materialize: F,
) -> anyhow::Result<IndexCounts>
where
    F: FnOnce(SourcePlan) -> Fut + Send + 'a,
    Fut: Future<Output = anyhow::Result<MaterializedSource>> + Send + 'a,
{
    let source_id = input.plan.route.source.source_id.clone();
    let previous = runtime.ledger.get_source(source_id.clone()).await?;
    // Upsert the source BEFORE job status: jobs.source_id references sources.
    // Otherwise a foreign-key error can mask the original source failure.
    let running_counts = previous
        .as_ref()
        .map(preserved_source_counts)
        .unwrap_or_else(empty_source_counts);
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Running,
            running_counts,
            previous.as_ref(),
        ))
        .await?;
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Leasing,
        "acquiring source lease",
    )
    .await?;
    let lease = runtime
        .ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("source:{}", source_id.0),
            owner_id: input.owner_id.to_string(),
            ttl_seconds: SOURCE_LEASE_TTL_SECONDS,
            job_id: Some(input.plan.job_id),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("source refresh already running for {}", source_id.0))?;
    let result = maintain(
        runtime.ledger.clone(),
        &lease,
        SOURCE_LEASE_TTL_SECONDS,
        input.execution.cancellation.clone(),
        async {
            match until_cancelled(input.execution, materialize(input.plan.clone())).await {
                Ok(materialized) => {
                    input.plan = materialized.plan.clone();
                    let result =
                        run_generation(runtime, input, emitter, &lease, previous.clone()).await;
                    drop(materialized);
                    result
                }
                Err(error) => Err(error),
            }
        },
    )
    .await;
    let summary = record_source_failure(runtime, input, emitter, previous.as_ref(), &result).await;
    let release = runtime
        .ledger
        .release_lease(lease.lease_id, input.owner_id.to_string())
        .await;
    let result = match (result, summary) {
        (result, Ok(())) => result,
        (Err(primary), Err(summary)) => {
            Err(primary.context(format!("source failure summary also failed: {summary:#}")))
        }
        (Ok(_), Err(summary)) => Err(summary),
    };
    merge_source_and_release(runtime, result, release).await
}

/// Renew independently of pipeline polling, including while a provider holds
/// SQLite writer admission. The final publication check remains authoritative:
/// renewal never reacquires an expired or stolen lease. Dropping the operation
/// also aborts its renewal task; normal completion joins it before lease release.
pub(in crate::source) async fn maintain<T>(
    ledger: Arc<dyn LedgerStore>,
    lease: &LeaseGuard,
    ttl_seconds: u64,
    cancellation: Option<tokio_util::sync::CancellationToken>,
    operation: impl Future<Output = T>,
) -> T {
    let mut release_on_cancel = ReleaseOnCancellation {
        ledger: ledger.clone(),
        lease: Some(lease.clone()),
    };
    let lease = lease.clone();
    let heartbeat = AbortOnDropHandle::new(tokio::spawn(async move {
        let period = Duration::from_secs(ttl_seconds)
            .div_f64(3.0)
            .max(Duration::from_millis(1));
        loop {
            tokio::time::sleep(period).await;
            match ledger
                .heartbeat_lease(lease.lease_id.clone(), lease.owner_id.clone(), ttl_seconds)
                .await
            {
                Ok(Some(_)) => {}
                Ok(None) => {
                    tracing::warn!(lease_key = %lease.lease_key, "source lease renewal lost ownership");
                    if let Some(cancel) = &cancellation {
                        cancel.cancel();
                    }
                    return;
                }
                Err(error) => {
                    tracing::warn!(lease_key = %lease.lease_key, error = %error,
                        "source lease renewal failed; canceling guarded work");
                    if let Some(cancel) = &cancellation {
                        cancel.cancel();
                        return;
                    }
                }
            }
        }
    }));
    let result = operation.await;
    heartbeat.abort();
    let _ = heartbeat.await;
    release_on_cancel.lease = None;
    result
}

/// A canceled publication future must not strand its finalizer lease. Normal
/// completion leaves release/error reporting to the caller; dropped futures
/// schedule an owner-checked release without waiting for the lease TTL.
struct ReleaseOnCancellation {
    ledger: Arc<dyn LedgerStore>,
    lease: Option<LeaseGuard>,
}

impl Drop for ReleaseOnCancellation {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let ledger = self.ledger.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = ledger.release_lease(lease.lease_id, lease.owner_id).await {
                    tracing::warn!(lease_key = %lease.lease_key, error = %error,
                        "canceled operation lease release failed; expiry will recover it");
                }
            });
        }
    }
}

/// Interrupt expensive pre-generation work without interrupting the generation
/// failure/cleanup path, which remains owned by run_generation.
pub(super) async fn until_cancelled<T>(
    execution: &SourceExecutionContext,
    operation: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    match execution.cancellation.as_ref() {
        Some(cancel) => tokio::select! {
            biased;
            () = cancel.cancelled() => Err(anyhow::anyhow!("source execution canceled or lease ownership lost")),
            result = operation => result,
        },
        None => operation.await,
    }
}

#[cfg(test)]
#[path = "lease_heartbeat_tests.rs"]
mod tests;
