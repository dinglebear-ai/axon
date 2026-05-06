//! `AcpSessionCache` — process-wide session store with background reaper.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use super::super::PermissionResponderMap;
use super::super::persistent_conn::AcpConnectionHandle;
use super::entry::CachedSession;
use super::{SESSION_CACHE, SESSION_HUNG_TURN_THRESHOLD};

/// Global cache of live ACP adapter sessions.
///
/// Keyed by `agent_key` (e.g. `"Claude:mcp=12345"`), same key used by
/// `get_or_create_acp_connection` in `pulse_chat.rs`.
pub struct AcpSessionCache {
    pub(super) sessions: DashMap<String, Arc<CachedSession>>,
    /// Maps ACP `session_id` → `agent_key` for reconnect lookups.
    pub(super) session_id_index: DashMap<String, String>,
    pub(super) reaper_started: std::sync::Once,
}

impl AcpSessionCache {
    pub(super) fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            session_id_index: DashMap::new(),
            reaper_started: std::sync::Once::new(),
        }
    }

    /// Ensure the background reaper task is running (idempotent).
    pub fn ensure_reaper(&self) {
        self.reaper_started.call_once(|| {
            tracing::info!(
                session_ttl_secs = super::SESSION_TTL.as_secs(),
                hung_threshold_secs = SESSION_HUNG_TURN_THRESHOLD.as_secs(),
                "acp: session cache reaper starting",
            );
            tokio::spawn(reaper_loop());
        });
    }

    /// Get an existing cached session by agent key.
    pub fn get(&self, agent_key: &str) -> Option<Arc<CachedSession>> {
        let entry = self.sessions.get(agent_key)?;
        let session = Arc::clone(entry.value());
        session.touch();
        Some(session)
    }

    /// Synchronous (non-touching) lookup by agent key.
    ///
    /// Used where we cannot `.await` (e.g. `send_or_buffer` fallback path).
    pub fn get_sync(&self, agent_key: &str) -> Option<Arc<CachedSession>> {
        let entry = self.sessions.get(agent_key)?;
        Some(Arc::clone(entry.value()))
    }

    /// Get a cached session by ACP session_id (for reconnect lookups).
    pub fn get_by_session_id(&self, session_id: &str) -> Option<Arc<CachedSession>> {
        let agent_key = self.session_id_index.get(session_id)?.value().clone();
        self.get(&agent_key)
    }

    /// Get a cached session by ACP session_id synchronously (for permission routing).
    pub fn get_by_session_id_sync(&self, session_id: &str) -> Option<Arc<CachedSession>> {
        let agent_key = self.session_id_index.get(session_id)?.value().clone();
        self.sessions.get(&agent_key).map(|e| Arc::clone(e.value()))
    }

    /// Insert or replace a cached session.
    ///
    /// After inserting, if the global session cap (`AXON_ACP_MAX_SESSIONS`,
    /// default 100, 0 = unlimited) is exceeded, the least-recently-used session
    /// is evicted (at most one per call — see `evict_if_over_cap`).
    ///
    /// ## Concurrency notes (documented, not fixed)
    ///
    /// - **Overshoot window:** between `sessions.insert()` and `evict_if_over_cap()`,
    ///   N concurrent writers may each observe `len <= cap` and skip eviction,
    ///   transiently overshooting the cap by up to `N` entries (where `N` is the
    ///   count of in-flight inserts). The next single insert will only evict one
    ///   victim, so the cache may run a few entries over `cap` until traffic
    ///   resumes. This is acceptable: ACP session inserts are rare (one per
    ///   distinct agent_key), and a global mutex would defeat `DashMap`'s
    ///   per-shard locking.
    /// - **Eviction scan is O(N):** `evict_if_over_cap` linearly scans all
    ///   entries to pick the LRU victim. At the default cap of 100 (and
    ///   realistic deployments well under 1000), this is negligible. If the
    ///   cap is raised into the tens of thousands, replace this with an
    ///   ordered index (e.g. `BTreeMap<Instant, String>`).
    pub fn insert(
        &self,
        agent_key: String,
        handle: Arc<AcpConnectionHandle>,
        permission_responders: PermissionResponderMap,
    ) -> Arc<CachedSession> {
        let session = Arc::new(CachedSession::new(handle, permission_responders));
        let replaced = self
            .sessions
            .insert(agent_key.clone(), Arc::clone(&session))
            .is_some();
        let cache_size = self.sessions.len();
        if replaced {
            tracing::info!(
                agent_key = %agent_key,
                cache_size,
                "acp: session inserted (replaced existing handle)"
            );
        } else {
            tracing::info!(
                agent_key = %agent_key,
                cache_size,
                "acp: session inserted"
            );
        }
        let cap = *super::MAX_SESSIONS;
        if cap > 0 {
            self.evict_if_over_cap(cap);
        }
        self.ensure_reaper();
        session
    }

    /// Evict at most ONE LRU session if the cache has exceeded `cap`.
    ///
    /// Finds the victim by scanning `last_active` across all sessions
    /// (O(N) — see notes on `insert`), clones its key, drops the iterator,
    /// then calls `self.remove()` (which also cleans `session_id_index`).
    /// Skips eviction if every entry was freshly inserted with an identical
    /// timestamp — extremely rare and safe to miss.
    ///
    /// **Contract:** This function evicts at most one entry per call. Callers
    /// that need to bring the cache under `cap` after a burst of concurrent
    /// inserts must call repeatedly (or rely on subsequent inserts to drain
    /// the overshoot one-by-one).
    ///
    /// **Caller guarantees `cap > 0`** — `insert()` checks this before calling.
    pub(super) fn evict_if_over_cap(&self, cap: usize) {
        if self.sessions.len() <= cap {
            return;
        }
        // Find the LRU key by scanning last_active under each entry's lock.
        // Iterator is dropped before remove() to avoid lock-order inversion.
        let lru_key: Option<String> = self
            .sessions
            .iter()
            .min_by_key(|e| {
                *e.value()
                    .last_active
                    .lock()
                    .expect("last_active mutex poisoned")
            })
            .map(|e| e.key().clone());

        if let Some(key) = lru_key {
            let cache_size = self.sessions.len();
            // Routine at-cap eviction is expected behavior, not a warning.
            tracing::info!(
                agent_key = %key,
                cache_size,
                cap,
                "acp: session evicted (global cap reached)"
            );
            self.remove(&key);
        }
    }

    /// Register a mapping from ACP session_id to agent_key.
    ///
    /// Called after the first turn establishes a session so that reconnecting
    /// clients can look up the adapter by the session_id they remember.
    pub fn register_session_id(&self, session_id: String, agent_key: String) {
        self.session_id_index.insert(session_id, agent_key);
    }

    /// Remove a session by agent key (e.g. on agent change).
    pub fn remove(&self, agent_key: &str) {
        if let Some((_, _session)) = self.sessions.remove(agent_key) {
            // Clean up session_id_index entries pointing to this agent_key.
            self.session_id_index.retain(|_, v| v.as_str() != agent_key);
            tracing::info!(
                agent_key = %agent_key,
                cache_size = self.sessions.len(),
                "acp: session removed"
            );
        }
    }

    /// Evict expired and hung sessions.
    ///
    /// M8: Uses `retain()` for single-pass eviction without intermediate
    /// Vec allocation. Expired/hung keys are collected for session_id_index
    /// cleanup in a second pass (index cleanup requires `self.remove()`
    /// which calls `session_id_index.retain()` internally).
    pub(super) async fn reap_expired(&self) {
        let hung_threshold = SESSION_HUNG_TURN_THRESHOLD;
        let mut to_remove = Vec::new();
        self.sessions.retain(|key, session| {
            if session.is_expired() {
                tracing::warn!(agent_key = %key, reason = "ttl_expired", "acp: session evicted");
                to_remove.push(key.clone());
                false
            } else if session.is_turn_hung(hung_threshold) {
                tracing::warn!(
                    agent_key = %key,
                    reason = "hung_turn",
                    threshold_secs = hung_threshold.as_secs(),
                    "acp: session evicted (hung turn)",
                );
                to_remove.push(key.clone());
                false
            } else {
                true
            }
        });
        // Clean up session_id_index entries for evicted agent keys.
        if !to_remove.is_empty() {
            self.session_id_index
                .retain(|_, agent_key| !to_remove.iter().any(|k| k == agent_key));
        }
    }

    /// Returns an iterator over all cached sessions.
    pub fn sessions_iter(&self) -> dashmap::iter::Iter<'_, String, Arc<CachedSession>> {
        self.sessions.iter()
    }

    /// Returns a list of all active agent keys in the cache.
    pub fn agent_keys(&self) -> Vec<String> {
        self.sessions.iter().map(|e| e.key().clone()).collect()
    }

    /// Number of cached sessions (for diagnostics).
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Background task that periodically reaps expired sessions.
pub(super) async fn reaper_loop() {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        SESSION_CACHE.reap_expired().await;
    }
}
