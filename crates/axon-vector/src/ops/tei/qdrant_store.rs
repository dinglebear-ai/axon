use crate::ops::qdrant::qdrant_base;
use axon_core::config::Config;
use axon_core::config::parse::tuning;
use axon_core::http::internal_service_http_client;
use axon_core::logging::{log_debug, log_info, log_warn};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

mod payload_indexes;
#[cfg(test)]
#[path = "qdrant_store_tests.rs"]
mod tests;
mod upsert;
use payload_indexes::ensure_payload_indexes;
pub use upsert::qdrant_upsert;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CollectionInitResult {
    pub(super) mode: VectorMode,
    pub(super) created_now: bool,
    pub(super) restore_indexing_threshold: bool,
}

/// Process-lifetime cache for collection vector schema.
///
/// `Named` hits are authoritative for the current process. `Unnamed` hits are
/// revalidated when hybrid search is enabled so long-running workers self-heal
/// after a collection migrates from legacy dense-only mode to named hybrid mode.
/// Uses `RwLock` (not `Mutex`) because after initial population most accesses are
/// read-only -- `RwLock` allows unlimited concurrent readers.
static COLLECTION_MODES: LazyLock<RwLock<HashMap<String, VectorMode>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Timestamps of the last successful mode-probe per collection cache key.
///
/// Used to rate-limit re-probes for `Unnamed` collections when hybrid search is
/// enabled — we re-probe at most every ~60s instead of on every embed/query. (P-M7)
static COLLECTION_MODE_LAST_PROBE: LazyLock<RwLock<HashMap<String, Instant>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn collection_mode_cache_key(cfg: &Config) -> String {
    format!(
        "{}\x1f{}",
        qdrant_base(cfg).trim_end_matches('/'),
        cfg.collection
    )
}

/// Return the cached `VectorMode` for `name`, or `None` if not yet initialized.
fn cached_vector_mode_key(key: &str) -> Option<VectorMode> {
    COLLECTION_MODES
        .read()
        .ok()
        .and_then(|map| map.get(key).copied())
}

/// Return `true` if we should skip a live Qdrant re-probe for this cache key.
///
/// `Unnamed` + hybrid-enabled: re-probe at most every 60s to preserve the
/// post-migrate self-healing behavior without paying a round-trip on every
/// embed/query. (P-M7)
fn cached_vector_mode_is_authoritative(cfg: &Config, key: &str, mode: VectorMode) -> bool {
    if !matches!(mode, VectorMode::Unnamed) {
        return true; // Named is permanently authoritative
    }
    if !cfg.hybrid_search_enabled {
        return true; // Hybrid disabled — no need to watch for schema upgrades
    }
    // Unnamed + hybrid: gate re-probes to at most once per 60s.
    const PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
    COLLECTION_MODE_LAST_PROBE
        .read()
        .ok()
        .and_then(|m| m.get(key).copied())
        .is_some_and(|last| last.elapsed() < PROBE_INTERVAL)
}

/// Record a successful mode-probe timestamp so the rate-limiter can skip future
/// probes within the window. (P-M7)
fn record_probe_time(key: &str) {
    if let Ok(mut m) = COLLECTION_MODE_LAST_PROBE.write() {
        m.insert(key.to_owned(), Instant::now());
    }
}

#[cfg(test)]
fn cached_vector_mode(name: &str) -> Option<VectorMode> {
    cached_vector_mode_key(name)
}

/// Store `mode` in the collection-mode cache for `name`.
fn cache_vector_mode_key(key: &str, mode: VectorMode) {
    match COLLECTION_MODES.write() {
        Ok(mut m) => {
            m.insert(key.to_owned(), mode);
        }
        Err(e) => {
            log_warn(&format!(
                "COLLECTION_MODES RwLock poisoned, cache write skipped for '{}': {e}",
                key
            ));
        }
    }
}

#[cfg(test)]
fn cache_vector_mode(name: &str, mode: VectorMode) {
    cache_vector_mode_key(name, mode);
}

/// Clear a specific entry from the collection mode cache.
///
/// Useful for long-running workers that need to re-detect collection schema
/// after a migration (e.g., Unnamed -> Named via `axon migrate`).
pub fn clear_collection_mode_cache(name: &str) {
    if let Ok(mut m) = COLLECTION_MODES.write() {
        m.remove(name);
        let suffix = format!("\x1f{name}");
        m.retain(|key, _| !key.ends_with(&suffix));
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
) -> Result<CollectionInitResult, Box<dyn Error>> {
    let cache_key = collection_mode_cache_key(cfg);
    if let Some(mode) = cached_vector_mode_key(&cache_key) {
        if cached_vector_mode_is_authoritative(cfg, &cache_key, mode) {
            return Ok(CollectionInitResult {
                mode,
                created_now: false,
                restore_indexing_threshold: false,
            });
        }
        log_info(&format!(
            "qdrant collection_mode_cache_revalidate collection={} cached_mode={:?} reason=hybrid_enabled",
            cfg.collection, mode
        ));
    }
    let init = ensure_collection_result(cfg, dim).await?;
    cache_vector_mode_key(&cache_key, init.mode);
    record_probe_time(&cache_key);
    Ok(init)
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
    let cache_key = collection_mode_cache_key(cfg);
    if let Some(mode) = cached_vector_mode_key(&cache_key) {
        if cached_vector_mode_is_authoritative(cfg, &cache_key, mode) {
            return Ok(mode);
        }
        log_info(&format!(
            "qdrant mode_probe_revalidate collection={} cached_mode={:?} reason=hybrid_enabled",
            cfg.collection, mode
        ));
    }
    let client = internal_service_http_client()?;
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
    cache_vector_mode_key(&cache_key, mode);
    record_probe_time(&cache_key);
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

/// Read `AXON_QDRANT_HNSW_ON_DISK` (default `false`) and
/// `AXON_QDRANT_QUANTIZATION_ALWAYS_RAM` (default `true`).
/// Lever B (axon_rust-o9y2): trade ~5-6 GiB RAM for higher query latency.
fn memory_settings() -> (bool, bool) {
    (
        tuning::qdrant_hnsw_on_disk(),
        tuning::qdrant_quantization_always_ram(),
    )
}

fn hnsw_build_settings() -> (usize, usize) {
    (tuning::qdrant_hnsw_m(), tuning::qdrant_hnsw_ef_construct())
}

fn bulk_indexing_thresholds() -> Option<(usize, usize)> {
    if !tuning::qdrant_bulk_load() {
        return None;
    }
    Some((
        tuning::qdrant_bulk_indexing_threshold_kb(),
        tuning::qdrant_indexing_threshold_kb(),
    ))
}

/// Ensure the collection exists and is configured with the right vector schema.
///
/// Returns the `VectorMode` that describes the collection after this call.
///
/// | Prior state | Action | Returns |
/// |-------------|--------|---------|
/// | Does not exist | Create with named `dense` + `bm42` sparse | `Named` |
/// | Exists, named `dense` | Ensure sparse; PATCH sparse/memory if needed | `Named` |
/// | Exists, unnamed dense | Log warning; leave unchanged | `Unnamed` |
#[cfg(test)]
pub(crate) async fn ensure_collection(
    cfg: &Config,
    dim: usize,
) -> Result<VectorMode, Box<dyn Error>> {
    Ok(ensure_collection_result(cfg, dim).await?.mode)
}

async fn ensure_collection_result(
    cfg: &Config,
    dim: usize,
) -> Result<CollectionInitResult, Box<dyn Error>> {
    let client = internal_service_http_client()?;
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
                maybe_patch_memory_settings(cfg, &body).await?;
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
        ensure_payload_indexes(cfg, Some(&body)).await?;
        return Ok(CollectionInitResult {
            mode,
            created_now: false,
            restore_indexing_threshold: false,
        });
    } else if get_status != StatusCode::NOT_FOUND {
        // 500, 401, 403, etc. -- do not silently fall through to collection creation.
        let body = get_resp.text().await.unwrap_or_default();
        return Err(format!(
            "Qdrant GET collection/{} failed: {} -- {}",
            cfg.collection, get_status, body
        )
        .into());
    }

    let (hnsw_on_disk, always_ram) = memory_settings();
    let (hnsw_m, hnsw_ef_construct) = hnsw_build_settings();
    let bulk_thresholds = bulk_indexing_thresholds();
    let mut create = serde_json::json!({
        // Memory-bounded large-collection recipe: keep the raw f32 vectors on
        // disk (they're only needed for rescore), but pin the small int8
        // quantized vectors AND the HNSW graph in RAM so search never pays a
        // cold mmap page-fault. Without `quantization.always_ram` the quantized
        // set is evictable by OS page-cache pressure (e.g. a concurrent crawl),
        // which intermittently spikes search latency from ~40ms to 20-30s and
        // trips the 30s internal HTTP client timeout. `vectors.on_disk` must
        // stay true here: with always_ram quantization the raw vectors don't
        // belong in RAM too, or a 3.95M-point collection needs ~19GB.
        // Set AXON_QDRANT_HNSW_ON_DISK=true / AXON_QDRANT_QUANTIZATION_ALWAYS_RAM=false
        // to trade latency for ~5-6GiB RSS savings (lever B, axon_rust-o9y2).
        "vectors": {
            "dense": {"size": dim, "distance": "Cosine", "on_disk": true}
        },
        "sparse_vectors": {
            "bm42": {"modifier": "idf"}
        },
        "hnsw_config": {
            "m": hnsw_m,
            "ef_construct": hnsw_ef_construct,
            "on_disk": hnsw_on_disk
        },
        "quantization_config": {
            "scalar": {
                "type": "int8",
                "quantile": 0.99,
                "always_ram": always_ram
            }
        }
    });
    if let Some((bulk_threshold, _restore_threshold)) = bulk_thresholds {
        create["optimizers_config"] = serde_json::json!({
            "indexing_threshold": bulk_threshold
        });
    }
    let resp = client.put(&url).json(&create).send().await?;
    let status = resp.status();
    let created_now = status != StatusCode::CONFLICT;
    if created_now {
        resp.error_for_status()?;
    }
    // Collection schema changed -- bump generation so any cached doc-chunk
    // entries from a prior incarnation become unreachable. (axon_rust-pmc)
    crate::cache::bump_generation(&cfg.collection);
    if created_now {
        log_info(&format!(
            "qdrant collection_created collection={} mode=Named hnsw_m={hnsw_m} hnsw_ef_construct={hnsw_ef_construct} hnsw_on_disk={hnsw_on_disk} always_ram={always_ram} bulk_indexing_threshold_kb={}",
            cfg.collection,
            bulk_thresholds
                .map(|(threshold, _)| threshold)
                .unwrap_or(20_000)
        ));
    } else {
        log_info(&format!(
            "qdrant collection_create_conflict collection={} mode=Named",
            cfg.collection
        ));
    }
    ensure_payload_indexes(cfg, None).await?;
    Ok(CollectionInitResult {
        mode: VectorMode::Named,
        created_now,
        restore_indexing_threshold: created_now && bulk_thresholds.is_some(),
    })
}

pub(super) async fn restore_bulk_indexing_threshold_after_load(
    cfg: &Config,
) -> Result<(), Box<dyn Error>> {
    let Some((_bulk_threshold, restore_threshold)) = bulk_indexing_thresholds() else {
        return Ok(());
    };
    let client = internal_service_http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    client
        .patch(&url)
        .json(&serde_json::json!({
            "optimizers_config": {"indexing_threshold": restore_threshold}
        }))
        .send()
        .await?
        .error_for_status()?;
    log_info(&format!(
        "qdrant collection_restored_indexing_threshold collection={} indexing_threshold_kb={restore_threshold}",
        cfg.collection
    ));
    Ok(())
}

/// PATCH an existing Named collection to add the `bm42` sparse vector config.
async fn patch_add_sparse(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = internal_service_http_client()?;
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
    // Schema patch may change which points are returned for some URL --
    // invalidate cached doc-chunk entries via generation bump. (axon_rust-pmc)
    crate::cache::bump_generation(&cfg.collection);
    log_info(&format!(
        "qdrant collection_patched_sparse collection={}",
        cfg.collection
    ));
    Ok(())
}

/// PATCH memory-pressure knobs on an existing Named collection when the desired
/// settings diverge from the current ones.
///
/// Applies `hnsw_config.on_disk` and `quantization_config.scalar.always_ram` from
/// the `AXON_QDRANT_HNSW_ON_DISK` / `AXON_QDRANT_QUANTIZATION_ALWAYS_RAM` env vars
/// (lever B for axon_rust-o9y2). Qdrant rebuilds the HNSW graph asynchronously after
/// an `on_disk` flip — the PATCH returns immediately and the rebuild runs in the
/// background, so this does not block the embed path.
async fn maybe_patch_memory_settings(
    cfg: &Config,
    body: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let (desired_hnsw_on_disk, desired_always_ram) = memory_settings();
    let current_hnsw_on_disk = body
        .pointer("/result/config/hnsw_config/on_disk")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let current_always_ram = body
        .pointer("/result/config/quantization_config/scalar/always_ram")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if current_hnsw_on_disk == desired_hnsw_on_disk && current_always_ram == desired_always_ram {
        return Ok(());
    }
    let client = internal_service_http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    client
        .patch(&url)
        .json(&serde_json::json!({
            "hnsw_config": {"on_disk": desired_hnsw_on_disk},
            "quantization_config": {
                "scalar": {"type": "int8", "quantile": 0.99, "always_ram": desired_always_ram}
            }
        }))
        .send()
        .await?
        .error_for_status()?;
    log_info(&format!(
        "qdrant collection_patched_memory collection={} hnsw_on_disk={desired_hnsw_on_disk} always_ram={desired_always_ram}",
        cfg.collection
    ));
    Ok(())
}
