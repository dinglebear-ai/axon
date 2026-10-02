//! Job-aware recovery and bounded disposal of private graph stages.
use axon_api::source::LifecycleStatus;
use axon_graph::stage::{GraphStage, GraphStageRecord};
use axon_jobs::boundary::JobStore;
use sqlx::SqlitePool;

pub(super) async fn sweep(pool: &SqlitePool, jobs: &dyn JobStore) {
    let records = match GraphStage::list(pool).await {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(error = %error, "graph stage cleanup inventory failed; retry next sweep");
            return;
        }
    };
    for record in records {
        let owner = match jobs.get(record.job_id).await {
            Ok(job) => job.map(|job| (job.status, job.attempt)),
            Err(error) => {
                tracing::warn!(stage_id = %record.stage_id, error = %error, "graph stage owner lookup failed; preserving stage");
                continue;
            }
        };
        if !eligible(&record, owner) {
            continue;
        }
        let ids = [record.stage_id.clone()];
        // One stage per operation prevents a corrupt path or filesystem failure
        // from starving other durable disposal records. The graph domain owns
        // path validation and refuses deletion while a stage owner holds its lock.
        if let Err(error) = GraphStage::mark_disposable_ids(pool, &ids).await {
            tracing::warn!(stage_id = %record.stage_id, error = %error, "graph stage disposal marking failed; retry next sweep");
            continue;
        }
        match GraphStage::reap(pool, &ids).await {
            Ok(0) => {}
            Ok(count) => {
                tracing::info!(stage_id = %record.stage_id, count, "private graph stage disposed")
            }
            Err(error) => {
                tracing::warn!(stage_id = %record.stage_id, error = %error, "graph stage disposal failed; durable record retained for retry")
            }
        }
    }
}

fn eligible(record: &GraphStageRecord, owner: Option<(LifecycleStatus, u32)>) -> bool {
    if matches!(record.state.as_str(), "activated" | "disposable") {
        return true;
    }
    let Some((status, attempt)) = owner else {
        return false;
    };
    attempt > record.attempt
        || (attempt == record.attempt
            && matches!(
                status,
                LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
                    | LifecycleStatus::Failed
                    | LifecycleStatus::Canceled
                    | LifecycleStatus::Expired
                    | LifecycleStatus::Skipped
            ))
}

#[cfg(test)]
#[path = "cleanup_stages_tests.rs"]
mod tests;
