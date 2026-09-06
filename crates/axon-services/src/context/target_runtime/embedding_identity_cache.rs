use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axon_core::config::Config;
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio::task::AbortHandle;

use super::{
    EMBEDDING_PROVIDER_ID, EmbeddingIdentity, derive_embedding_identity,
    embedding_identity_cache_key,
};

#[derive(Debug, Clone)]
struct CachedEmbeddingIdentity {
    identity: EmbeddingIdentity,
    expires_at: Instant,
}

#[derive(Default)]
struct EmbeddingIdentityCache {
    entries: HashMap<String, CachedEmbeddingIdentity>,
    in_flight: HashMap<String, InFlightIdentityProbe>,
}

struct InFlightIdentityProbe {
    sender: watch::Sender<Option<EmbeddingIdentity>>,
    started_at: Instant,
    abort_handle: Option<AbortHandle>,
}

static EMBEDDING_IDENTITY_CACHE: OnceLock<Mutex<EmbeddingIdentityCache>> = OnceLock::new();
const EMBEDDING_IDENTITY_DURABLE_TTL: Duration = Duration::from_secs(30 * 60);
const EMBEDDING_IDENTITY_STALE_AFTER: Duration = Duration::from_secs(120);

fn embedding_identity_cache() -> &'static Mutex<EmbeddingIdentityCache> {
    EMBEDDING_IDENTITY_CACHE.get_or_init(|| Mutex::new(EmbeddingIdentityCache::default()))
}

/// Resolve the embedding model + dimensions from the live TEI endpoint (`/info`
/// for `model_id`, a probe embed for dimensions). Builds a probe provider seeded
/// with the fallback identity purely to issue the derivation requests. Falls
/// back to the configured defaults when the provider is unreachable, so a
/// fire-and-forget CLI enqueue or an offline TEI never blocks store construction.
pub(crate) async fn resolve_embedding_identity(cfg: &Config) -> EmbeddingIdentity {
    let cache_key = embedding_identity_cache_key(cfg);
    loop {
        let receiver = match claim_embedding_identity_resolution(&cache_key, cfg.clone()) {
            IdentityResolutionClaim::Cached(identity) => return identity,
            IdentityResolutionClaim::Wait(receiver) => receiver,
        };
        if let Some(identity) = wait_for_embedding_identity(receiver).await {
            return identity;
        }
        // A supervised probe was aborted or panicked. Re-enter the claim path;
        // it removes the closed/stale record and starts a fresh probe.
    }
}

/// Resolve TEI identity with a process-independent SQLite cache. Short-lived
/// CLI reads therefore pay the `/info` + probe-embed cost only when the cache
/// is cold, while long-lived processes retain the faster in-memory singleflight
/// above. Fallback identities are deliberately never persisted.
pub(crate) async fn resolve_embedding_identity_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
) -> EmbeddingIdentity {
    let cache_key = embedding_identity_cache_key(cfg);
    if let Some(identity) = load_durable_embedding_identity(pool, &cache_key).await {
        return identity;
    }

    let identity = resolve_embedding_identity(cfg).await;
    if identity.verified
        && let Err(error) = persist_durable_embedding_identity(pool, &cache_key, &identity).await
    {
        tracing::warn!(%error, "failed to persist verified embedding identity cache");
    }
    identity
}

async fn load_durable_embedding_identity(
    pool: &SqlitePool,
    cache_key: &str,
) -> Option<EmbeddingIdentity> {
    let ttl_ms = i64::try_from(EMBEDDING_IDENTITY_DURABLE_TTL.as_millis()).unwrap_or(i64::MAX);
    let cutoff = chrono::Utc::now().timestamp_millis().saturating_sub(ttl_ms);
    let row = sqlx::query_as::<_, (String, i64)>(
        "SELECT model, dimensions FROM provider_identity_cache \
         WHERE cache_key = ? AND provider_kind = 'embedding' AND updated_at >= ?",
    )
    .bind(cache_key)
    .bind(cutoff)
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some((model, dimensions))) => {
            let dimensions = u32::try_from(dimensions).ok()?;
            (!model.trim().is_empty() && dimensions > 0).then_some(EmbeddingIdentity {
                model,
                dimensions,
                verified: true,
            })
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "failed to read durable embedding identity cache");
            None
        }
    }
}

async fn persist_durable_embedding_identity(
    pool: &SqlitePool,
    cache_key: &str,
    identity: &EmbeddingIdentity,
) -> Result<(), sqlx::Error> {
    debug_assert!(identity.verified);
    sqlx::query(
        "INSERT INTO provider_identity_cache \
         (cache_key, provider_kind, provider_id, model, dimensions, updated_at) \
         VALUES (?, 'embedding', ?, ?, ?, ?) \
         ON CONFLICT(cache_key) DO UPDATE SET \
           provider_kind = excluded.provider_kind, \
           provider_id = excluded.provider_id, \
           model = excluded.model, \
           dimensions = excluded.dimensions, \
           updated_at = excluded.updated_at",
    )
    .bind(cache_key)
    .bind(EMBEDDING_PROVIDER_ID)
    .bind(&identity.model)
    .bind(i64::from(identity.dimensions))
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await?;
    Ok(())
}

/// Invalidate the cached identity and any negative result for this exact TEI
/// configuration. Configuration reload callers can use this after changing a
/// model or endpoint instead of waiting for the bounded TTL to elapse.
pub fn invalidate_embedding_identity_cache(cfg: &Config) {
    let key = embedding_identity_cache_key(cfg);
    if let Ok(mut cache) = embedding_identity_cache().lock() {
        cache.entries.remove(&key);
    }
}

enum IdentityResolutionClaim {
    Cached(EmbeddingIdentity),
    Wait(watch::Receiver<Option<EmbeddingIdentity>>),
}

/// Atomically return a cached identity or join/create the one probe currently
/// allowed for a configuration key. The actual probe is detached from callers:
/// cancelling one query cannot strand the other callers behind its request.
fn claim_embedding_identity_resolution(key: &str, cfg: Config) -> IdentityResolutionClaim {
    let mut cache = embedding_identity_cache()
        .lock()
        .expect("embedding identity cache mutex poisoned");
    if let Some(entry) = cache.entries.get(key) {
        if entry.expires_at > Instant::now() {
            return IdentityResolutionClaim::Cached(entry.identity.clone());
        }
    }
    cache.entries.remove(key);
    if let Some(probe) = cache.in_flight.get(key) {
        if !probe.sender.is_closed() && probe.started_at.elapsed() < EMBEDDING_IDENTITY_STALE_AFTER
        {
            return IdentityResolutionClaim::Wait(probe.sender.subscribe());
        }
    }
    if let Some(stale) = cache.in_flight.remove(key)
        && let Some(handle) = stale.abort_handle
    {
        handle.abort();
    }

    let (sender, receiver) = watch::channel(None);
    cache.in_flight.insert(
        key.to_string(),
        InFlightIdentityProbe {
            sender,
            started_at: Instant::now(),
            abort_handle: None,
        },
    );
    let abort_handle = spawn_embedding_identity_probe(key.to_string(), cfg);
    if let Some(probe) = cache.in_flight.get_mut(key) {
        probe.abort_handle = Some(abort_handle);
    }
    IdentityResolutionClaim::Wait(receiver)
}

fn spawn_embedding_identity_probe(cache_key: String, cfg: Config) -> AbortHandle {
    let probe = tokio::spawn(async move { derive_embedding_identity(&cfg).await });
    let abort_handle = probe.abort_handle();
    tokio::spawn(async move {
        // The supervisor is independent from the abortable probe and therefore
        // always removes the map entry after success, panic, or cancellation.
        let derived = probe.await;
        let sender = {
            let mut cache = embedding_identity_cache()
                .lock()
                .expect("embedding identity cache mutex poisoned");
            let sender = cache.in_flight.remove(&cache_key).map(|probe| probe.sender);
            if let Ok((identity, ttl)) = &derived {
                cache.entries.insert(
                    cache_key.clone(),
                    CachedEmbeddingIdentity {
                        identity: identity.clone(),
                        expires_at: Instant::now() + *ttl,
                    },
                );
            }
            sender
        };
        if let (Ok((identity, _)), Some(sender)) = (derived, sender) {
            sender.send_replace(Some(identity));
        }
    });
    abort_handle
}

async fn wait_for_embedding_identity(
    mut receiver: watch::Receiver<Option<EmbeddingIdentity>>,
) -> Option<EmbeddingIdentity> {
    loop {
        if let Some(identity) = receiver.borrow().clone() {
            return Some(identity);
        }
        if receiver.changed().await.is_err() {
            return None;
        }
    }
}

#[cfg(test)]
pub(crate) fn abort_embedding_identity_probe(cfg: &Config) {
    let key = embedding_identity_cache_key(cfg);
    if let Ok(mut cache) = embedding_identity_cache().lock()
        && let Some(probe) = cache.in_flight.remove(&key)
        && let Some(handle) = probe.abort_handle
    {
        handle.abort();
    }
}
