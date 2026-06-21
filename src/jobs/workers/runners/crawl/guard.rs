use std::future::Future;

use tokio_util::sync::CancellationToken;

/// Typed failure from the guarded crawl phase. Internal helpers return this;
/// `run_crawl_job` boxes it at the runner boundary via [`CrawlGuardError::into_boxed`].
pub(super) enum CrawlGuardError {
    Canceled,
    /// Wall-clock budget (`crawl_job_timeout_secs`) elapsed; carries the limit
    /// for the operator-facing message.
    Timeout(i64),
    Engine(String),
}

impl CrawlGuardError {
    pub(super) fn into_boxed(self) -> Box<dyn std::error::Error + Send + Sync> {
        match self {
            Self::Canceled => "crawl canceled".into(),
            Self::Timeout(secs) => {
                format!("crawl job exceeded crawl_job_timeout_secs ({secs}s) — aborted").into()
            }
            Self::Engine(msg) => msg.into(),
        }
    }
}

/// Result of racing the engine future against cancel + deadline. The interrupted
/// engine future is **not** dropped by [`race_engine_guards`] — it is borrowed,
/// so the caller can signal shutdown and then drain it (running its cleanup)
/// before it is finally dropped.
pub(super) enum GuardOutcome<T> {
    Completed(T),
    Canceled,
    TimedOut,
}

/// The crawl job's shared wall-clock budget: an absolute `deadline` (`None`
/// disables the timeout) plus the configured `secs` for the abort message.
/// Built once in `run_crawl_job` and applied to both the engine and the backfill.
#[derive(Clone, Copy)]
pub(super) struct CrawlBudget {
    pub deadline: Option<tokio::time::Instant>,
    pub secs: i64,
}

/// Race `engine` against an optional cancel token and an optional wall-clock
/// `deadline`, borrowing (not consuming) the engine future. When `cancel_token`
/// or `deadline` is `None` that arm waits forever, so the engine runs unbounded.
pub(super) async fn race_engine_guards<Fut>(
    engine: &mut Fut,
    cancel_token: Option<&CancellationToken>,
    deadline: Option<tokio::time::Instant>,
) -> GuardOutcome<Fut::Output>
where
    Fut: Future + Unpin,
{
    tokio::select! {
        v = &mut *engine => GuardOutcome::Completed(v),
        _ = wait_for_cancel(cancel_token) => GuardOutcome::Canceled,
        _ = wait_until_deadline(deadline) => GuardOutcome::TimedOut,
    }
}

async fn wait_for_cancel(cancel_token: Option<&CancellationToken>) {
    match cancel_token {
        Some(token) => token.cancelled().await,
        None => std::future::pending::<()>().await,
    }
}

async fn wait_until_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(dl) => tokio::time::sleep_until(dl).await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
