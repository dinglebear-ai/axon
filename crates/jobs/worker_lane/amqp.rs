use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::jobs::common::open_amqp_connection_and_channel;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use lapin::options::{BasicConsumeOptions, BasicQosOptions};
use lapin::types::FieldTable;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::delivery::claim_delivery;
use super::{ProcessFn, STALE_SWEEP_INTERVAL_SECS, WorkerConfig, sweep_stale_jobs};

/// Result of polling the consumer + inflight set. Distinguishes between a real
/// idle timeout (no deliveries for `STALE_SWEEP_INTERVAL_SECS`) and an inflight
/// job completing without a new delivery being ready. Only the former should
/// trigger a stale-job sweep.
#[derive(Debug)]
pub(crate) enum PollOutcome {
    /// A delivery arrived from the AMQP consumer.
    Delivery(Box<Result<lapin::message::Delivery, lapin::Error>>),
    /// The consumer stream ended (broker closed the channel).
    ConsumerClosed,
    /// No delivery arrived within the sweep interval — trigger a stale sweep.
    IdleTimeout,
    /// An inflight job completed but no new delivery is ready. Re-poll without
    /// sweeping — the sweep cadence is maintained by the idle timeout path.
    InflightCompleted,
}

/// Open an AMQP connection, set QoS, declare a consumer, and log startup.
/// Returns `(Connection, Channel, Consumer)` ready to receive deliveries.
async fn setup_amqp_consumer(
    cfg: &Config,
    wc: &WorkerConfig,
    lane: usize,
) -> Result<(lapin::Connection, lapin::Channel, lapin::Consumer), Box<dyn std::error::Error>> {
    let (conn, ch) = open_amqp_connection_and_channel(cfg, &wc.queue_name).await?;

    // Tell the broker to only push one unacked message at a time per consumer,
    // preventing a single lane from buffering more work than it can process.
    ch.basic_qos(1, BasicQosOptions::default()).await?;

    let tag = format!("{}-{lane}", wc.consumer_tag_prefix);
    let consumer = ch
        .basic_consume(
            wc.queue_name.as_str().into(),
            tag.as_str().into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    log_info(&format!(
        "{} worker lane={lane} listening on queue={} concurrency={}",
        wc.job_kind, wc.queue_name, wc.lane_count
    ));

    Ok((conn, ch, consumer))
}

pub(crate) async fn poll_next_delivery(
    inflight: &mut FuturesUnordered<Pin<Box<dyn Future<Output = ()>>>>,
    consumer: &mut lapin::Consumer,
) -> PollOutcome {
    if inflight.is_empty() {
        return match tokio::time::timeout(
            Duration::from_secs(STALE_SWEEP_INTERVAL_SECS),
            consumer.next(),
        )
        .await
        {
            Ok(Some(delivery)) => PollOutcome::Delivery(Box::new(delivery)),
            Ok(None) => PollOutcome::ConsumerClosed,
            Err(_elapsed) => PollOutcome::IdleTimeout,
        };
    }
    tokio::select! {
        maybe_done = inflight.next() => {
            if maybe_done.is_some() {
                // An inflight job completed but no new delivery is ready yet.
                // Signal InflightCompleted so the outer loop re-polls WITHOUT
                // triggering a stale sweep — the sweep cadence is maintained
                // solely by IdleTimeout.
                return PollOutcome::InflightCompleted;
            }
            // FuturesUnordered returned None — set is now empty. Fall through
            // to a normal consumer poll with sweep-interval timeout.
            match tokio::time::timeout(
                Duration::from_secs(STALE_SWEEP_INTERVAL_SECS),
                consumer.next(),
            )
            .await
            {
                Ok(Some(delivery)) => PollOutcome::Delivery(Box::new(delivery)),
                Ok(None) => PollOutcome::ConsumerClosed,
                Err(_elapsed) => PollOutcome::IdleTimeout,
            }
        }
        delivery = tokio::time::timeout(Duration::from_secs(STALE_SWEEP_INTERVAL_SECS), consumer.next()) => {
            match delivery {
                Ok(Some(d)) => PollOutcome::Delivery(Box::new(d)),
                Ok(None) => PollOutcome::ConsumerClosed,
                Err(_elapsed) => PollOutcome::IdleTimeout,
            }
        }
    }
}

enum DeliveryOutcome {
    Delivery(Box<lapin::message::Delivery>),
    Continue,
    Break,
}

async fn parse_delivery_result(
    outcome: PollOutcome,
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
    lane: usize,
) -> DeliveryOutcome {
    match outcome {
        PollOutcome::Delivery(result) => match *result {
            Ok(d) => DeliveryOutcome::Delivery(Box::new(d)),
            Err(e) => {
                log_warn(&format!(
                    "{} worker lane={lane} AMQP delivery error: {e}",
                    wc.job_kind
                ));
                DeliveryOutcome::Continue
            }
        },
        PollOutcome::ConsumerClosed => DeliveryOutcome::Break,
        PollOutcome::IdleTimeout => {
            sweep_stale_jobs(cfg, pool, wc, "amqp", lane).await;
            DeliveryOutcome::Continue
        }
        PollOutcome::InflightCompleted => DeliveryOutcome::Continue,
    }
}

async fn close_amqp_lane(conn: lapin::Connection, ch: lapin::Channel, wc: &WorkerConfig) {
    if let Err(e) = ch.close(200, "lane exit".into()).await {
        log_debug(&format!(
            "amqp ch_close failed queue={} error={e}",
            wc.queue_name
        ));
    }
    if let Err(e) = conn.close(200, "lane exit".into()).await {
        log_debug(&format!(
            "amqp conn_close failed queue={} error={e}",
            wc.queue_name
        ));
    }
}

/// Generic AMQP consumer lane. Listens for job IDs on the queue, claims them,
/// and dispatches to `process_fn` concurrently using `FuturesUnordered` with a
/// semaphore for backpressure. Runs stale sweeps on idle timeout.
pub(crate) async fn run_amqp_lane(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: Arc<tokio::sync::Semaphore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, ch, mut consumer) = setup_amqp_consumer(cfg, wc, lane).await?;

    // ProcessFn returns !Send futures; the lane runs on a single task so Send
    // is not required.
    let mut inflight: FuturesUnordered<Pin<Box<dyn Future<Output = ()>>>> = FuturesUnordered::new();

    // Sweep interval used in the full-capacity backpressure path so that
    // watchdog sweeps keep firing even when all semaphore permits are held.
    let mut sweep_interval = tokio::time::interval(Duration::from_secs(STALE_SWEEP_INTERVAL_SECS));
    sweep_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    sweep_interval.tick().await; // consume the immediate first tick

    loop {
        // If all permits are consumed, block until at least one in-flight job
        // completes OR the sweep interval fires.  Without the select! here,
        // sweeps stop firing for the entire duration of any saturated burst,
        // which can span hours for long-running jobs.
        if semaphore.available_permits() == 0 && !inflight.is_empty() {
            tokio::select! {
                _ = inflight.next() => {}
                _ = sweep_interval.tick() => {
                    sweep_stale_jobs(cfg, &pool, wc, "amqp", lane).await;
                }
            }
            continue;
        }

        let poll_outcome = poll_next_delivery(&mut inflight, &mut consumer).await;
        let delivery = match parse_delivery_result(poll_outcome, cfg, &pool, wc, lane).await {
            DeliveryOutcome::Delivery(d) => *d,
            DeliveryOutcome::Continue => continue,
            DeliveryOutcome::Break => break,
        };

        if let Some(job_fut) =
            claim_delivery(delivery, cfg, &pool, wc, lane, process_fn, &semaphore).await?
        {
            inflight.push(job_fut);
        }
    }

    // Drain any remaining in-flight jobs before exiting.
    while inflight.next().await.is_some() {}

    // Explicitly close channel and connection so RabbitMQ cleans up immediately
    // rather than waiting for the TCP timeout.
    close_amqp_lane(conn, ch, wc).await;

    Err(format!(
        "{} worker lane={lane} AMQP consumer stream ended unexpectedly",
        wc.job_kind
    )
    .into())
}
