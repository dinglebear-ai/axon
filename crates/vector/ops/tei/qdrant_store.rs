use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_base};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::sync::{OnceLock, RwLock};

#[cfg(test)]
mod tests;

/// Describes how a Qdrant collection's vectors are configured.
///
/// - `Unnamed`: legacy single unnamed dense vector (`"vectors": {"size": N}`)
///   -- hybrid search is disabled, `/points/search` is used.
/// - `Named`: named `dense` + named `bm42` sparse vectors
///   -- hybrid search is enabled, `/points/query` with RRF is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorMode {
    Unnamed,
    Named,
}

/// Process-lifetime cache: entries are never evicted. A process restart is required
/// to pick up collection schema changes (e.g., migration from Unnamed to Named).
/// Uses `RwLock` (not `Mutex`) because after initial population all accesses are
/// read-only -- `RwLock` allows unlimited concurrent readers.
static COLLECTION_MODES: OnceLock<RwLock<HashMap<String, VectorMode>>> = OnceLock::new();

/// Return the cached `VectorMode` for `name`, or `None` if not yet initialized.
fn cached_vector_mode(name: &str) -> Option<VectorMode> {
    COLLECTION_MODES
        .get()
        .and_then(|m| m.read().ok())
        .and_then(|map| map.get(name).copied())
}

/// Store `mode` in the collection-mode cache for `name`.
fn cache_vector_mode(name: &str, mode: VectorMode) {
    let map = COLLECTION_MODES.get_or_init(|| RwLock::new(HashMap::new()));
    match map.write() {
        Ok(mut m) => {
            m.insert(name.to_owned(), mode);
        }
        Err(e) => {
            log_warn(&format!(
                "COLLECTION_MODES RwLock poisoned, cache write skipped for '{}': {e}",
                name
            ));
        }
    }
}

/// Clear a specific entry from the collection mode cache.
///
/// Useful for long-running workers that need to re-detect collection schema
/// after a migration (e.g., Unnamed -> Named via `axon migrate`).
#[expect(dead_code, reason = "reserved for long-running workers post-migration")]
pub(crate) fn clear_collection_mode_cache(name: &str) {
    if let Some(map) = COLLECTION_MODES.get()
        && let Ok(mut m) = map.write()
    {
        m.remove(name);
    }
}

/// Return the `VectorMode` for `cfg.collection`, initializing the Qdrant collection
/// if this is the first call for that collection in this process.
///
/// Subsequent calls return the cached mode without hitting Qdrant.
///
/// # Concurrency note
/// There is a TOCTOU window: two concurrent first-time callers can both see `None`
/// from `cached_vector_mode` and both call `ensure_collection`. This is safe because
/// `ensure_collection` is idempotent -- the second PUT gets a 409 CONFLICT that is
/// explicitly ignored, and both callers end up with a consistent `VectorMode`.
pub(super) async fn collection_init_or_cached(
    cfg: &Config,
    dim: usize,
) -> Result<VectorMode, Box<dyn Error>> {
    if let Some(mode) = cached_vector_mode(&cfg.collection) {
        return Ok(mode);
    }
    let mode = ensure_collection(cfg, dim).await?;
    cache_vector_mode(&cfg.collection, mode);
    Ok(mode)
}

/// Return the `VectorMode` for `cfg.collection` by inspecting the live Qdrant schema.
///
/// Used by search-only paths (query/ask) where `collection_init_or_cached` may not
/// have been called yet. Checks cache first; falls back to a GET if not cached.
///
/// # Failure policy
/// If Qdrant is unreachable, returns a non-2xx response, or returns malformed JSON,
/// this function returns an error instead of guessing a mode. Guessing `Unnamed`
/// on probe failures can misroute Named collections to `/points/search`, which
/// Qdrant rejects.
pub(crate) async fn get_or_fetch_vector_mode(cfg: &Config) -> Result<VectorMode, Box<dyn Error>> {
    if let Some(mode) = cached_vector_mode(&cfg.collection) {
        return Ok(mode);
    }
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    const MODE_PROBE_MAX_ATTEMPTS: usize = 3;
    let mut resp = None;
    let mut last_transport_error = None;
    for attempt in 1..=MODE_PROBE_MAX_ATTEMPTS {
        match client.get(&url).send().await {
            Ok(r) => {
                let status = r.status();
                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if retryable && attempt < MODE_PROBE_MAX_ATTEMPTS {
                    let backoff_ms = 150u64 * (1u64 << (attempt - 1));
                    log_warn(&format!(
                        "qdrant mode probe retrying collection='{}' status={} attempt={}/{} backoff_ms={}",
                        cfg.collection, status, attempt, MODE_PROBE_MAX_ATTEMPTS, backoff_ms
                    ));
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    continue;
                }
                resp = Some(r);
                break;
            }
            Err(e) => {
                last_transport_error = Some(e.to_string());
                if attempt < MODE_PROBE_MAX_ATTEMPTS {
                    let backoff_ms = 150u64 * (1u64 << (attempt - 1));
                    log_warn(&format!(
                        "qdrant mode probe transport retry collection='{}' attempt={}/{} backoff_ms={} err={}",
                        cfg.collection, attempt, MODE_PROBE_MAX_ATTEMPTS, backoff_ms, e
                    ));
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    continue;
                }
            }
        }
    }

    let Some(resp) = resp else {
        return Err(format!(
            "qdrant mode probe failed for collection '{}' after {} attempts: {}",
            cfg.collection,
            MODE_PROBE_MAX_ATTEMPTS,
            last_transport_error.unwrap_or_else(|| "transport error".to_string())
        )
        .into());
    };

    let status = resp.status();

    // 404 -> explicit not-found error.
    // Do not silently assume Unnamed mode; callers need a clear operator signal.
    if status == StatusCode::NOT_FOUND {
        return Err(format!(
            "qdrant mode probe returned 404 for collection '{}': collection not found",
            cfg.collection
        )
        .into());
    }

    // Non-2xx (except 404 handled above) -> fail explicitly.
    if !status.is_success() {
        return Err(format!(
            "qdrant mode probe returned {} for collection '{}'",
            status, cfg.collection
        )
        .into());
    }

    // HTTP 200 -> parse and cache the authoritative mode.
    let mode = match resp.json::<serde_json::Value>().await {
        Ok(body) => detect_vector_mode(&body),
        Err(e) => {
            return Err(format!(
                "qdrant mode probe returned malformed JSON for collection '{}': {e}",
                cfg.collection
            )
            .into());
        }
    };
    cache_vector_mode(&cfg.collection, mode);
    Ok(mode)
}

/// Infer `VectorMode` from a Qdrant collection GET response body.
fn detect_vector_mode(body: &serde_json::Value) -> VectorMode {
    if body
        .pointer("/result/config/params/vectors/dense")
        .is_some()
    {
        VectorMode::Named
    } else {
        VectorMode::Unnamed
    }
}

/// Check that the existing collection's dense vector dimension matches `expected_dim`.
///
/// Returns `Ok(())` when dimensions match or when the stored dimension cannot be
/// determined (ambiguous schema is not an error -- only a confirmed mismatch is).
fn validate_existing_dim(
    body: &serde_json::Value,
    mode: VectorMode,
    expected_dim: usize,
    collection: &str,
) -> Result<(), Box<dyn Error>> {
    let stored = match mode {
        VectorMode::Unnamed => body.pointer("/result/config/params/vectors/size"),
        VectorMode::Named => body.pointer("/result/config/params/vectors/dense/size"),
    };
    if let Some(serde_json::Value::Number(n)) = stored
        && let Some(stored_dim) = n.as_u64()
    {
        let stored_dim = stored_dim as usize;
        if stored_dim != expected_dim {
            return Err(format!(
                "collection '{}' has dense dim={} but current embedder uses dim={} \
                 -- set AXON_COLLECTION to a new name to re-index",
                collection, stored_dim, expected_dim
            )
            .into());
        }
    }
    Ok(())
}

/// Creates keyword payload indexes on commonly-queried fields.
///
/// These indexes are required by the Qdrant `/facet` endpoint used by the
/// `domains` and `sources` MCP actions.  The operation is idempotent --
/// Qdrant returns HTTP 200 when the index already exists.
///
/// All index PUT requests are issued concurrently (they are independent
/// and idempotent), avoiding 5 sequential round-trips on cold collection init.
async fn ensure_payload_indexes(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );

    // Build all index requests as futures and run them concurrently.
    let keyword_fields = [
        "url",
        "domain",
        "source_type",
        "gh_file_language",
        "chunking_method",
    ];
    type IndexFut<'a> = std::pin::Pin<
        Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'a>,
    >;
    let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(keyword_fields.len() + 1);

    for field in &keyword_fields {
        let url = index_url.clone();
        futures.push(Box::pin(async move {
            client
                .put(&url)
                .json(&serde_json::json!({
                    "field_name": field,
                    "field_schema": "keyword"
                }))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }));
    }

    // datetime index for scraped_at range queries (--since / --before)
    let datetime_url = index_url;
    futures.push(Box::pin(async move {
        client
            .put(&datetime_url)
            .json(&serde_json::json!({
                "field_name": "scraped_at",
                "field_schema": "datetime"
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));

    let results = futures_util::future::join_all(futures).await;
    for result in results {
        result.map_err(|e| -> Box<dyn Error> { e })?;
    }
    Ok(())
}

/// Ensure the collection exists and is configured with the right vector schema.
///
/// Returns the `VectorMode` that describes the collection after this call.
///
/// | Prior state | Action | Returns |
/// |-------------|--------|---------|
/// | Does not exist | Create with named `dense` + `bm42` sparse | `Named` |
/// | Exists, named `dense` | Ensure sparse; PATCH to add `bm42` if missing | `Named` |
/// | Exists, unnamed dense | Log warning; leave unchanged | `Unnamed` |
pub(super) async fn ensure_collection(
    cfg: &Config,
    dim: usize,
) -> Result<VectorMode, Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);

    let get_resp = client.get(&url).send().await?;
    let get_status = get_resp.status();

    if get_status.is_success() {
        let body: serde_json::Value = get_resp.json().await?;
        let mode = detect_vector_mode(&body);
        validate_existing_dim(&body, mode, dim, &cfg.collection)?;
        match mode {
            VectorMode::Named => {
                let has_sparse = body
                    .pointer("/result/config/params/sparse_vectors/bm42")
                    .is_some();
                if !has_sparse {
                    patch_add_sparse(cfg).await?;
                }
            }
            VectorMode::Unnamed => {
                log_warn(&format!(
                    "collection '{}' uses legacy unnamed dense vectors; \
                     hybrid search is disabled for this collection. \
                     To enable, set AXON_COLLECTION to a new name and re-index.",
                    cfg.collection
                ));
            }
        }
        log_debug(&format!(
            "qdrant collection_exists collection={} mode={:?}",
            cfg.collection, mode
        ));
        ensure_payload_indexes(cfg).await?;
        return Ok(mode);
    } else if get_status != StatusCode::NOT_FOUND {
        // 500, 401, 403, etc. -- do not silently fall through to collection creation.
        let body = get_resp.text().await.unwrap_or_default();
        return Err(format!(
            "Qdrant GET collection/{} failed: {} -- {}",
            cfg.collection, get_status, body
        )
        .into());
    }

    let create = serde_json::json!({
        "vectors": {
            "dense": {"size": dim, "distance": "Cosine"}
        },
        "sparse_vectors": {
            "bm42": {"modifier": "idf"}
        }
    });
    let resp = client.put(&url).json(&create).send().await?;
    if resp.status() != StatusCode::CONFLICT {
        resp.error_for_status()?;
    }
    log_info(&format!(
        "qdrant collection_created collection={} mode=Named",
        cfg.collection
    ));
    ensure_payload_indexes(cfg).await?;
    Ok(VectorMode::Named)
}

/// PATCH an existing Named collection to add the `bm42` sparse vector config.
async fn patch_add_sparse(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    client
        .patch(&url)
        .json(&serde_json::json!({
            "sparse_vectors": {
                "bm42": {"modifier": "idf"}
            }
        }))
        .send()
        .await?
        .error_for_status()?;
    log_info(&format!(
        "qdrant collection_patched_sparse collection={}",
        cfg.collection
    ));
    Ok(())
}

/// Upsert points into Qdrant with automatic batching and retry.
///
/// Points are split into batches of `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 256)
/// and each batch is retried up to 3 times with exponential backoff (500ms, 1s, 2s).
/// Uses `?wait=true` so the call blocks until Qdrant has committed the write.
pub(super) async fn qdrant_upsert(
    cfg: &Config,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    if points.is_empty() {
        return Ok(());
    }
    let client = http_client()?;
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
        let mut last_err = String::new();
        let mut succeeded = false;
        for attempt in 1..=3u32 {
            match client
                .put(&url)
                .json(&serde_json::json!({"points": batch}))
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
    Ok(())
}
