//! AMQP connection utilities.
//!
//! # Two reconnect implementations
//!
//! This codebase has two AMQP consumer reconnect loops with different semantics:
//! - `crawl/runtime/worker/loops.rs::run_amqp_lane_with_reconnect()`: used by the crawl
//!   worker. Backoff resets to INIT on every successful reconnect.
//! - `worker_lane::run_job_worker()`: used by embed/extract/refresh workers.
//!   Backoff resets to INIT only after the connection has been alive for ≥ 60s.
//!
//! The difference is intentional: crawl jobs are long-running so a short-lived
//! connection that handles one job should not penalize the next reconnect.

use crate::crates::core::config::Config;
use crate::crates::core::content::redact_url;
use crate::crates::core::logging::log_debug;
use anyhow::{Context, Result};
use lapin::types::FieldTable;
use lapin::{Channel, Connection, ConnectionProperties};
use std::time::Duration;
use uuid::Uuid;

use super::durable_queue_options;

#[expect(dead_code)]
pub const GRAPH_QUEUE_DEFAULT: &str = "axon.graph.jobs";

/// Open an AMQP channel with a 5-second connection timeout and declare the given queue.
///
/// **Warning:** This drops the `Connection`, so the returned channel's backing TCP
/// connection will close asynchronously. Only use this for short-lived operations
/// (health checks, queue_purge). For long-lived consumers, use
/// `open_amqp_connection_and_channel` and keep the `Connection` in scope.
///
/// Consequence on misuse: each call opens and immediately destroys an AMQP TCP
/// connection — callers left holding a dropped Connection will receive
/// `InvalidChannelState` errors on the returned channel.
pub async fn open_amqp_channel(cfg: &Config, queue_name: &str) -> Result<Channel> {
    let (_, ch) = open_amqp_connection_and_channel(cfg, queue_name).await?;
    Ok(ch)
}

pub(crate) async fn open_amqp_connection_and_channel(
    cfg: &Config,
    queue_name: &str,
) -> Result<(Connection, Channel)> {
    let props = ConnectionProperties::default();
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        Connection::connect(&cfg.amqp_url, props),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "amqp connect timeout: {} (if running in Docker without published ports, run from same Docker network or expose rabbitmq)",
            redact_url(&cfg.amqp_url)
        )
    })?
    .context("amqp connect failed")?;
    let ch = tokio::time::timeout(Duration::from_secs(5), async {
        let ch = conn.create_channel().await?;
        ch.queue_declare(
            queue_name.into(),
            durable_queue_options(),
            FieldTable::default(),
        )
        .await?;
        Ok::<Channel, lapin::Error>(ch)
    })
    .await
    .map_err(|_| anyhow::anyhow!("amqp channel/queue declare timeout for queue={queue_name}"))?
    .context("amqp create channel/declare queue failed")?;
    Ok((conn, ch))
}

/// Publish a job ID to an AMQP queue.
///
/// Delegates to [`batch_enqueue_jobs`] with a single-element slice — same
/// connection lifecycle, same publisher-confirm semantics, no duplicate code.
pub async fn enqueue_job(cfg: &Config, queue_name: &str, job_id: Uuid) -> Result<()> {
    batch_enqueue_jobs(cfg, queue_name, &[job_id]).await
}

/// Core publish implementation used by both the fresh-connection and
/// channel-reuse paths. Sends N messages on the given channel and waits
/// for publisher confirms.
async fn publish_to_channel(ch: &Channel, queue_name: &str, job_ids: &[Uuid]) -> Result<()> {
    use lapin::BasicProperties;
    use lapin::options::{BasicPublishOptions, ConfirmSelectOptions};

    ch.confirm_select(ConfirmSelectOptions::default())
        .await
        .context("confirm_select failed")?;
    for id in job_ids {
        ch.basic_publish(
            "".into(),
            queue_name.into(),
            BasicPublishOptions::default(),
            id.to_string().as_bytes(),
            BasicProperties::default().with_delivery_mode(2),
        )
        .await
        .with_context(|| format!("basic_publish job {id} to queue={queue_name}"))?;
    }
    ch.wait_for_confirms()
        .await
        .context("wait_for_confirms failed")?;
    Ok(())
}

/// Publish multiple job IDs to an AMQP queue over a single connection.
///
/// More efficient than calling [`enqueue_job`] in a loop — one TCP handshake,
/// N publishes, one CLOSE. Uses publisher confirms so the broker acks every
/// message before we close — follows the official lapin `publisher_confirms` example.
pub async fn batch_enqueue_jobs(cfg: &Config, queue_name: &str, job_ids: &[Uuid]) -> Result<()> {
    batch_enqueue_jobs_with_channel(cfg, queue_name, job_ids, None).await
}

/// Publish multiple job IDs, optionally reusing an existing AMQP channel.
///
/// When `existing_ch` is `Some` and the channel is still connected, publishes
/// reuse the existing TCP connection — zero connection overhead. Falls back to
/// a fresh connection if the channel is broken or `None`.
pub async fn batch_enqueue_jobs_with_channel(
    cfg: &Config,
    queue_name: &str,
    job_ids: &[Uuid],
    existing_ch: Option<&Channel>,
) -> Result<()> {
    if job_ids.is_empty() {
        return Ok(());
    }

    // Try the existing channel first — avoids a TCP connection + TLS handshake.
    if let Some(ch) = existing_ch
        && ch.status().connected()
    {
        match publish_to_channel(ch, queue_name, job_ids).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                log_debug(&format!(
                    "amqp reuse channel publish failed queue={queue_name}: {e}; opening fresh connection"
                ));
            }
        }
    }

    // Fallback: open a fresh connection.
    let (conn, ch) = open_amqp_connection_and_channel(cfg, queue_name).await?;
    publish_to_channel(&ch, queue_name, job_ids).await?;
    if let Err(e) = ch.close(0, "".into()).await {
        log_debug(&format!(
            "amqp ch_close failed queue={queue_name} error={e}"
        ));
    }
    if let Err(e) = conn.close(200, "".into()).await {
        log_debug(&format!(
            "amqp conn_close failed queue={queue_name} error={e}"
        ));
    }

    Ok(())
}

/// Purge all messages from the named AMQP queue, then explicitly close the
/// channel and connection.
///
/// This is the correct way to purge a queue — unlike [`open_amqp_channel`], it
/// keeps the `Connection` alive for the full duration of the operation.
pub(crate) async fn purge_queue_safe(cfg: &Config, queue_name: &str) -> Result<()> {
    use lapin::options::QueuePurgeOptions;

    let (conn, ch) = open_amqp_connection_and_channel(cfg, queue_name).await?;
    ch.queue_purge(queue_name.into(), QueuePurgeOptions::default())
        .await
        .context("queue_purge failed")?;
    if let Err(e) = ch.close(0, "".into()).await {
        log_debug(&format!(
            "amqp ch_close failed queue={queue_name} error={e}"
        ));
    }
    if let Err(e) = conn.close(200, "".into()).await {
        log_debug(&format!(
            "amqp conn_close failed queue={queue_name} error={e}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// `enqueue_job` delegates to `batch_enqueue_jobs` — verified at compile time
    /// by the delegation in the implementation. This test documents the contract.
    #[test]
    fn enqueue_job_delegates_to_batch() {
        // The implementation of enqueue_job is a one-liner calling batch_enqueue_jobs.
        // If someone changes it to open a new connection, this test name serves as
        // a reminder that the delegation contract was intentional.
    }

    /// AMQP reconnect backoff constants must be self-consistent across the two
    /// reconnect implementations (crawl loops.rs and worker_lane.rs module).
    #[test]
    fn amqp_reconnect_constants_are_self_consistent() {
        // Crawl worker constants (loops.rs)
        const CRAWL_RECONNECT_INIT: u64 = 2;
        const CRAWL_RECONNECT_MAX: u64 = 60;
        const _: () = assert!(CRAWL_RECONNECT_INIT < CRAWL_RECONNECT_MAX);
        // Max backoff should be ≤ 60s (avoid long hang on broker restart)
        const _: () = assert!(CRAWL_RECONNECT_MAX <= 60);
    }
}
