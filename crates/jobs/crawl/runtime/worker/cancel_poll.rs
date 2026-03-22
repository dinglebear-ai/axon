//! Redis-based crawl-job cancellation polling.
//!
//! `poll_cancel_key` is raced against the active crawl future inside
//! `run_active_crawl_job`. When the cancel key is detected, the crawl is
//! gracefully shut down via the spider control layer.

use std::time::Duration;

use redis::AsyncCommands;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};

/// Maximum number of reconnect attempts before giving up on cancel polling.
const CANCEL_POLL_MAX_RECONNECTS: u32 = 5;

/// Polls Redis until the cancel key is found, then returns.
/// Does an immediate first poll (no sleep before the first check), then polls
/// every 3 seconds. On connection failure, retries with bounded exponential
/// backoff (up to `CANCEL_POLL_MAX_RECONNECTS` attempts). After exhausting
/// retries, parks forever — tokio::select! will still complete via the crawl future.
///
/// Fail-safe: never false-cancels; if Redis is unreachable the crawl continues.
pub(super) async fn poll_cancel_key(cfg: &Config, id: Uuid) {
    let key = format!("axon:crawl:cancel:{id}");
    let mut conn = match connect_cancel_redis(cfg, id).await {
        Some(c) => c,
        None => {
            std::future::pending::<()>().await;
            unreachable!("pending() never resolves");
        }
    };

    // Immediate first poll — don't wait 3s before checking.
    if poll_cancel_once(&mut conn, &key).await {
        return;
    }

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let result =
            tokio::time::timeout(Duration::from_secs(3), conn.get::<_, Option<String>>(&key)).await;
        match result {
            Ok(Ok(Some(_))) => return,
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                log_warn(&format!(
                    "crawl cancel poll: Redis GET failed for job {id}: {e}; attempting reconnect"
                ));
                match reconnect_cancel_redis(cfg, id).await {
                    Some(new_conn) => conn = new_conn,
                    None => {
                        std::future::pending::<()>().await;
                        unreachable!("pending() never resolves");
                    }
                }
            }
            Err(_) => {
                log_warn(&format!(
                    "crawl cancel poll: Redis GET timed out for job {id}; attempting reconnect"
                ));
                match reconnect_cancel_redis(cfg, id).await {
                    Some(new_conn) => conn = new_conn,
                    None => {
                        std::future::pending::<()>().await;
                        unreachable!("pending() never resolves");
                    }
                }
            }
        }
    }
}

/// Single non-blocking cancel key check. Returns `true` if cancel key is set.
async fn poll_cancel_once(conn: &mut redis::aio::MultiplexedConnection, key: &str) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(3), conn.get::<_, Option<String>>(key)).await,
        Ok(Ok(Some(_)))
    )
}

/// Open a Redis connection for cancel polling.
async fn connect_cancel_redis(cfg: &Config, id: Uuid) -> Option<redis::aio::MultiplexedConnection> {
    let Ok(client) = redis::Client::open(cfg.redis_url.clone()) else {
        log_warn(&format!(
            "crawl cancel poll: failed to open Redis client for job {id}; cancellation disabled"
        ));
        return None;
    };
    match tokio::time::timeout(
        Duration::from_secs(3),
        client.get_multiplexed_async_connection(),
    )
    .await
    {
        Ok(Ok(conn)) => Some(conn),
        _ => {
            log_warn(&format!(
                "crawl cancel poll: Redis connect failed for job {id}; cancellation disabled"
            ));
            None
        }
    }
}

/// Reconnect to Redis with bounded exponential backoff.
/// Returns `None` after exhausting `CANCEL_POLL_MAX_RECONNECTS` attempts.
async fn reconnect_cancel_redis(
    cfg: &Config,
    id: Uuid,
) -> Option<redis::aio::MultiplexedConnection> {
    for attempt in 0..CANCEL_POLL_MAX_RECONNECTS {
        let backoff = Duration::from_secs(1 << attempt.min(4)); // 1s, 2s, 4s, 8s, 16s
        tokio::time::sleep(backoff).await;
        if let Some(conn) = connect_cancel_redis(cfg, id).await {
            log_info(&format!(
                "crawl cancel poll: Redis reconnected for job {id} after {} attempt(s)",
                attempt + 1
            ));
            return Some(conn);
        }
    }
    log_warn(&format!(
        "crawl cancel poll: Redis reconnect failed after {CANCEL_POLL_MAX_RECONNECTS} attempts for job {id}; cancellation disabled"
    ));
    None
}

#[cfg(test)]
mod tests {
    use super::poll_cancel_key;
    use crate::crates::jobs::common::{resolve_test_redis_url, test_config};
    use redis::AsyncCommands;
    use std::error::Error;
    use std::time::Duration;
    use uuid::Uuid;

    #[tokio::test]
    async fn cancel_key_set_triggers_poll_completion() -> Result<(), Box<dyn Error>> {
        let Some(redis_url) = resolve_test_redis_url() else {
            return Ok(());
        };
        let id = Uuid::new_v4();
        let key = format!("axon:crawl:cancel:{id}");

        let client = redis::Client::open(redis_url.clone())?;
        let mut conn = client.get_multiplexed_async_connection().await?;
        conn.set_ex::<_, _, ()>(&key, "1", 60).await?;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = redis_url;

        // Immediate first-poll finds the key → future completes well under 5s.
        let result = tokio::time::timeout(Duration::from_secs(5), poll_cancel_key(&cfg, id)).await;
        assert!(
            result.is_ok(),
            "set cancel key must trigger poll completion"
        );

        let _: () = conn.del(&key).await?;
        Ok(())
    }

    #[tokio::test]
    async fn cancel_key_absent_parks_poll() -> Result<(), Box<dyn Error>> {
        let Some(redis_url) = resolve_test_redis_url() else {
            return Ok(());
        };
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = redis_url;
        let id = Uuid::new_v4();

        // No key set — after the immediate first-poll misses, the loop sleeps 3s.
        // 200ms timeout fires before the 3s sleep completes.
        let result =
            tokio::time::timeout(Duration::from_millis(200), poll_cancel_key(&cfg, id)).await;
        assert!(result.is_err(), "absent cancel key must park the poller");
        Ok(())
    }

    #[tokio::test]
    async fn cancel_key_unreachable_redis_fails_safe() -> Result<(), Box<dyn Error>> {
        // No env var needed — port 1 is always unreachable (ECONNREFUSED).
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = "redis://127.0.0.1:1".to_string();
        let id = Uuid::new_v4();

        // connect_cancel_redis returns None → poll_cancel_key calls pending() → parks forever.
        let result = tokio::time::timeout(Duration::from_secs(5), poll_cancel_key(&cfg, id)).await;
        assert!(result.is_err(), "unreachable Redis must park, never cancel");
        Ok(())
    }
}
