//! Qdrant point upsert with batching and retry.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::{log_debug, log_warn};
use crate::vector::ops::qdrant::{env_usize_clamped, qdrant_base};
use std::error::Error;

/// Wire shape for a Qdrant `/points` PUT (upsert) request.
///
/// Built once per batch before the retry loop — borrows `points` for the lifetime
/// of the batch slice rather than re-allocating a `serde_json::Value` on every
/// retry attempt. (P-M1)
#[derive(serde::Serialize)]
struct UpsertBody<'a> {
    points: &'a [serde_json::Value],
}

/// Upsert points into Qdrant with automatic batching and retry.
///
/// Points are split into batches of `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 256)
/// and each batch is retried up to 3 times with exponential backoff (500ms, 1s, 2s).
/// Uses `?wait=true` so the call blocks until Qdrant has committed the write.
pub(crate) async fn qdrant_upsert(
    cfg: &Config,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    if points.is_empty() {
        return Ok(());
    }
    let client = internal_service_http_client()?;
    let upsert_batch_size = env_usize_clamped("AXON_QDRANT_UPSERT_BATCH_SIZE", 256, 1, 4096);
    let url = format!(
        "{}/collections/{}/points?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    log_debug(&format!(
        "qdrant upsert_start point_count={} collection={}",
        points.len(),
        cfg.collection
    ));
    for batch in points.chunks(upsert_batch_size) {
        // Build the request body once per batch, before the retry loop, to avoid
        // re-allocating a serde_json::Value on every retry attempt. (P-M1)
        let body = UpsertBody { points: batch };
        let mut last_err = String::new();
        let mut succeeded = false;
        for attempt in 1..=3u32 {
            match client
                .put(&url)
                .json(&body)
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                Ok(_) => {
                    succeeded = true;
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < 3 {
                        let backoff_ms = 500u64 * (1u64 << (attempt - 1));
                        log_warn(&format!(
                            "qdrant upsert attempt {attempt}/3 failed (collection={}): {e} — retrying in {backoff_ms}ms",
                            cfg.collection
                        ));
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    }
                }
            }
        }
        if !succeeded {
            return Err(format!(
                "qdrant upsert failed after 3 attempts (collection={}): {last_err}",
                cfg.collection
            )
            .into());
        }
    }
    // All batches succeeded -- bump generation once per upsert call so
    // cached doc-chunk entries reflect the new contents on next read.
    // (axon_rust-pmc)
    crate::vector::cache::bump_generation(&cfg.collection);
    Ok(())
}
