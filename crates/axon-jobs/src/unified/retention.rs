//! Differentiated, config-driven retention (R1-03).
//!
//! `docs/pipeline-unification/runtime/job-contract.md`'s "Retention" section
//! specifies different windows per data category (terminal job rows, detailed
//! events, failed-job events, artifacts, provider health) rather than the one
//! `older_than` cutoff `cleanup_jobs` (`unified/control.rs`) accepts on every
//! table alike. [`RetentionCutoffs`] carries the per-category cutoff
//! timestamps (derived from `Config::jobs_retention_*`, each independently
//! overridable via `[jobs.retention]`/env), and
//! [`SqliteUnifiedJobStore::run_retention_sweep`] applies them in one pass:
//!
//! 1. terminal `jobs` rows older than `terminal` (children cascade via FK)
//! 2. `job_events` belonging to *non-failed* jobs older than `event`
//! 3. `job_events` belonging to *failed* jobs older than `failed_event`
//!    (kept longer for postmortem evidence)
//! 4. `provider_reservations` older than `provider_health`
//! 5. `job_artifacts` older than `artifact`
//!
//! Called on a config-driven interval from the watchdog loop
//! (`workers/watchdog.rs`), independent of the on-demand `cleanup_jobs`
//! request path used by CLI/MCP `jobs cleanup`.

use axon_api::source::*;
use axon_core::config::Config;

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::unified_codec::*;

/// Per-category retention cutoffs, each a `now - N days` timestamp.
#[derive(Debug, Clone)]
pub(crate) struct RetentionCutoffs {
    pub terminal: Timestamp,
    pub event: Timestamp,
    pub failed_event: Timestamp,
    pub provider_health: Timestamp,
    pub artifact: Timestamp,
}

impl RetentionCutoffs {
    pub(crate) fn from_config(cfg: &Config) -> Self {
        let now = chrono::Utc::now();
        let cutoff = |days: i64| Timestamp::from(now - chrono::Duration::days(days.max(1)));
        Self {
            terminal: cutoff(cfg.jobs_retention_terminal_days),
            event: cutoff(cfg.jobs_retention_event_days),
            failed_event: cutoff(cfg.jobs_retention_failed_event_days),
            provider_health: cutoff(cfg.jobs_retention_provider_health_days),
            artifact: cutoff(cfg.jobs_retention_artifact_days),
        }
    }
}

impl SqliteUnifiedJobStore {
    /// Apply [`RetentionCutoffs`] across every differentiated category in one
    /// sweep. Best-effort per category — a failure in one delete is
    /// propagated (the caller logs and continues on the next tick) rather
    /// than partially applying and silently swallowing the rest.
    pub(crate) async fn run_retention_sweep(
        &self,
        cutoffs: &RetentionCutoffs,
    ) -> Result<JobCleanupResult> {
        // 1. Terminal job rows (cascades job_attempts/job_stages/job_events/
        // job_heartbeats/provider_reservations/job_artifacts via FK).
        let jobs_pruned = sqlx::query(
            "DELETE FROM jobs
             WHERE status IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')
               AND updated_at < ?",
        )
        .bind(cutoffs.terminal.0.as_str())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?
        .rows_affected();

        // 2. Detailed events for non-failed jobs, shorter retention.
        let events_pruned_non_failed = sqlx::query(
            "DELETE FROM job_events
             WHERE timestamp < ?
               AND job_id NOT IN (SELECT job_id FROM jobs WHERE status = 'failed')",
        )
        .bind(cutoffs.event.0.as_str())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?
        .rows_affected();

        // 3. Events for failed jobs, longer retention (postmortem evidence).
        let events_pruned_failed = sqlx::query(
            "DELETE FROM job_events
             WHERE timestamp < ?
               AND job_id IN (SELECT job_id FROM jobs WHERE status = 'failed')",
        )
        .bind(cutoffs.failed_event.0.as_str())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?
        .rows_affected();

        // 4. Provider capacity/health history.
        let reservations_pruned =
            sqlx::query("DELETE FROM provider_reservations WHERE updated_at < ?")
                .bind(cutoffs.provider_health.0.as_str())
                .execute(&self.pool)
                .await
                .map_err(sql_error)?
                .rows_affected();

        // 5. Artifacts.
        let artifacts_pruned = sqlx::query("DELETE FROM job_artifacts WHERE created_at < ?")
            .bind(cutoffs.artifact.0.as_str())
            .execute(&self.pool)
            .await
            .map_err(sql_error)?
            .rows_affected();

        Ok(JobCleanupResult {
            matched: jobs_pruned,
            deleted: jobs_pruned,
            dry_run: false,
            warnings: Vec::new(),
            jobs_pruned,
            events_pruned: events_pruned_non_failed + events_pruned_failed,
            heartbeats_pruned: 0,
            attempts_pruned: 0,
            stages_pruned: 0,
            reservations_pruned,
            artifacts_pruned,
        })
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
