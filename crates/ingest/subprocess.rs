use std::future::Future;
use std::process::Output;
use std::time::Duration;

/// Default timeout for subprocess calls (git clone, yt-dlp, etc.).
/// Prevents a hung process from blocking a worker lane indefinitely.
/// 5 minutes is generous for shallow clones and subtitle downloads.
pub const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum file size accepted before reading into memory (50 MiB).
/// Shared across file, wiki, and VTT file guards.
pub const MAX_INGEST_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Run a pre-built `tokio::process::Command` with a timeout.
///
/// Returns the raw `Output` on success. On timeout or spawn failure, returns
/// an error with `context` embedded in the message for diagnostics.
pub async fn run_command_with_timeout(
    child: impl Future<Output = std::io::Result<Output>>,
    timeout: Duration,
    context: &str,
) -> Result<Output, anyhow::Error> {
    tokio::time::timeout(timeout, child)
        .await
        .map_err(|_| anyhow::anyhow!("{context} timed out after {}s", timeout.as_secs()))?
        .map_err(|e| anyhow::anyhow!("{context}: process failed to start: {e}"))
}
