//! Qdrant point upsert with batching and retry.

use crate::ops::qdrant::{env_usize_clamped, qdrant_base};
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use axon_core::logging::{log_debug, log_warn};
use futures_util::stream::{self, StreamExt};
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

async fn upsert_batch_with_retry(
    client: &reqwest::Client,
    url: &str,
    collection: &str,
    batch: &[serde_json::Value],
) -> Result<(), String> {
    // Build the request body once per batch, before the retry loop, to avoid
    // re-allocating a serde_json::Value on every retry attempt. (P-M1)
    let body = UpsertBody { points: batch };
    let mut last_err = String::new();
    for attempt in 1..=3u32 {
        match client
            .put(url)
            .json(&body)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = e.to_string();
                if attempt < 3 {
                    let backoff_ms = 500u64 * (1u64 << (attempt - 1));
                    log_warn(&format!(
                        "qdrant upsert attempt {attempt}/3 failed (collection={collection} batch_points={}): {e} — retrying in {backoff_ms}ms",
                        batch.len()
                    ));
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }
    Err(format!(
        "qdrant upsert failed after 3 attempts (collection={collection} batch_points={}): {last_err}",
        batch.len()
    ))
}

/// Upsert points into Qdrant with automatic batching, bounded parallelism, and retry.
///
/// Points are split into batches of `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 1024)
/// and up to `AXON_QDRANT_UPSERT_PARALLELISM` (default 1) requests are sent at
/// once. Each batch is retried up to 3 times with exponential backoff (500ms, 1s,
/// 2s). Uses `?wait=true` so each request blocks until Qdrant has committed that
/// batch.
pub async fn qdrant_upsert(
    cfg: &Config,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    if points.is_empty() {
        return Ok(());
    }
    let client = internal_service_http_client()?;
    let upsert_batch_size = env_usize_clamped("AXON_QDRANT_UPSERT_BATCH_SIZE", 1024, 1, 4096);
    let upsert_parallelism = env_usize_clamped("AXON_QDRANT_UPSERT_PARALLELISM", 1, 1, 16);
    let url = format!(
        "{}/collections/{}/points?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    let collection = cfg.collection.clone();
    let batch_count = points.len().div_ceil(upsert_batch_size);
    log_debug(&format!(
        "qdrant upsert_start point_count={} collection={} batch_size={} batch_count={} parallelism={}",
        points.len(),
        cfg.collection,
        upsert_batch_size,
        batch_count,
        upsert_parallelism
    ));
    let batches = points
        .chunks(upsert_batch_size)
        .map(|batch| batch.to_vec())
        .collect::<Vec<_>>();
    let results = stream::iter(batches)
        .map(|batch| {
            let client = client.clone();
            let url = url.clone();
            let collection = collection.clone();
            async move { upsert_batch_with_retry(&client, &url, &collection, &batch).await }
        })
        .buffer_unordered(upsert_parallelism)
        .collect::<Vec<_>>()
        .await;
    if let Some(err) = results.into_iter().find_map(Result::err) {
        return Err(err.into());
    }
    // All batches succeeded -- bump generation once per upsert call so
    // cached doc-chunk entries reflect the new contents on next read.
    // (axon_rust-pmc)
    crate::cache::bump_generation(&cfg.collection);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[allow(unsafe_code)]
    #[tokio::test]
    #[serial_test::serial]
    async fn qdrant_upsert_splits_into_configured_batches() {
        let server = MockServer::start_async().await;
        let upsert = server
            .mock_async(|when, then| {
                when.method(PUT)
                    .path("/collections/upsert_batch_test/points")
                    .query_param("wait", "true");
                then.status(200)
                    .json_body(serde_json::json!({"result": {"operation_id": 1}, "status": "ok"}));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "upsert_batch_test".to_string();
        let points = (0..5)
            .map(|i| serde_json::json!({"id": i, "vector": [0.1, 0.2], "payload": {}}))
            .collect::<Vec<_>>();

        let saved_batch = std::env::var("AXON_QDRANT_UPSERT_BATCH_SIZE").ok();
        let saved_parallel = std::env::var("AXON_QDRANT_UPSERT_PARALLELISM").ok();
        unsafe {
            std::env::set_var("AXON_QDRANT_UPSERT_BATCH_SIZE", "2");
            std::env::set_var("AXON_QDRANT_UPSERT_PARALLELISM", "2");
        }
        let result = qdrant_upsert(&cfg, &points).await;
        unsafe {
            match saved_batch {
                Some(v) => std::env::set_var("AXON_QDRANT_UPSERT_BATCH_SIZE", v),
                None => std::env::remove_var("AXON_QDRANT_UPSERT_BATCH_SIZE"),
            }
            match saved_parallel {
                Some(v) => std::env::set_var("AXON_QDRANT_UPSERT_PARALLELISM", v),
                None => std::env::remove_var("AXON_QDRANT_UPSERT_PARALLELISM"),
            }
        }

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(upsert.calls_async().await, 3);
    }
}
