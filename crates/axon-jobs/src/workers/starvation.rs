use sqlx::SqlitePool;

/// Priority-aware interactive-lane starvation SLO.
///
/// `interactive` priority is meant to preempt background/bulk work, so a
/// queued interactive job that has waited longer than `slo_ms` with no
/// interactive job currently running is a fairness violation. `slo_ms <= 0`
/// disables the check.
pub(super) async fn detect_interactive_starvation(pool: &SqlitePool, slo_ms: i64) -> bool {
    if slo_ms <= 0 {
        return false;
    }
    let running: i64 = match sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE priority = 'interactive' AND status = 'running'",
    )
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(error = %error, "interactive starvation watchdog: running-count query failed");
            return false;
        }
    };
    if running > 0 {
        return false;
    }
    let oldest_queued_at: Option<String> = match sqlx::query_scalar(
        "SELECT updated_at FROM jobs
         WHERE priority = 'interactive' AND status = 'queued'
         ORDER BY updated_at ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!(error = %error, "interactive starvation watchdog: oldest-queued query failed");
            return false;
        }
    };
    let Some(oldest_queued_at) = oldest_queued_at else {
        return false;
    };
    let Ok(oldest_at) = chrono::DateTime::parse_from_rfc3339(&oldest_queued_at) else {
        tracing::warn!(
            oldest_queued_at,
            "interactive starvation watchdog: unparseable queued timestamp"
        );
        return false;
    };
    let waited_ms = (chrono::Utc::now() - oldest_at.with_timezone(&chrono::Utc)).num_milliseconds();
    if waited_ms < slo_ms {
        return false;
    }
    tracing::warn!(
        waited_secs = waited_ms / 1000,
        slo_secs = slo_ms / 1000,
        "interactive job starvation SLO breached: queued interactive work with none running"
    );
    true
}

#[cfg(test)]
#[path = "starvation_tests.rs"]
mod tests;
