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
    pub fn insert(
        &self,
        agent_key: String,
        handle: Arc<AcpConnectionHandle>,
        permission_responders: PermissionResponderMap,
    ) -> Arc<CachedSession> {
        let session = Arc::new(CachedSession::new(handle, permission_responders));
        self.sessions.insert(agent_key, Arc::clone(&session));
        self.ensure_reaper();
        session
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
        }
    }

    /// Evict expired and hung sessions.
    pub(super) async fn reap_expired(&self) {
        // Pass 1: clone keys + Arc refs (sync — releases DashMap shard locks immediately)
        let candidates: Vec<(String, Arc<CachedSession>)> = self
            .sessions
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        // Pass 2: check expiry and liveness — no DashMap locks held
        let hung_threshold = SESSION_HUNG_TURN_THRESHOLD;
        let mut to_remove = Vec::new();
        for (key, session) in &candidates {
            if session.is_expired() {
                tracing::info!(context = "acp_cache", key = %key, "evicting expired session");
                to_remove.push(key.clone());
            } else if session.is_turn_hung(hung_threshold) {
                tracing::warn!(
                    context = "acp_cache",
                    key = %key,
                    threshold_secs = hung_threshold.as_secs(),
                    "evicting session with hung turn",
                );
                to_remove.push(key.clone());
            }
        }
        for key in &to_remove {
            self.remove(key);
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
