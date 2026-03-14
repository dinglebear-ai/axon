//! Global ACP session cache — adapters survive WebSocket disconnects.
//!
//! `SESSION_CACHE` is a process-wide singleton that holds `AcpConnectionHandle`
//! instances keyed by agent key. When a WebSocket disconnects, the adapter stays
//! alive in the cache. On reconnect, the client resumes the same adapter and
//! receives any events buffered during the disconnect.
//!
//! Idle sessions are reaped after `SESSION_TTL` (default 30 minutes).

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

use super::PermissionResponderMap;
use super::persistent_conn::AcpConnectionHandle;

/// Default idle TTL before a cached session is evicted.
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// Hard cap on message count per session replay buffer. Secondary guard against
/// pathological cases with many tiny messages (primary limit is byte-based).
const MAX_REPLAY_BUFFER: usize = 4096;

/// Maximum cumulative byte size of the replay buffer per session (4 MiB).
/// Primary memory bound — a single `TurnResult` can be many KB, so counting
/// messages alone is insufficient.
const MAX_REPLAY_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// Process-wide ACP session cache.
pub static SESSION_CACHE: std::sync::LazyLock<AcpSessionCache> =
    std::sync::LazyLock::new(AcpSessionCache::new);

/// A cached ACP adapter session.
pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    last_active: std::sync::Mutex<Instant>,
    /// Serialized WS JSON messages buffered while no client is connected.
    replay_buffer: std::sync::Mutex<Vec<String>>,
    /// Cumulative byte size of all messages in `replay_buffer`.
    replay_buffer_bytes: std::sync::Mutex<usize>,
}

impl CachedSession {
    fn new(
        handle: Arc<AcpConnectionHandle>,
        permission_responders: PermissionResponderMap,
    ) -> Self {
        Self {
            handle,
            permission_responders,
            last_active: std::sync::Mutex::new(Instant::now()),
            replay_buffer: std::sync::Mutex::new(Vec::new()),
            replay_buffer_bytes: std::sync::Mutex::new(0),
        }
    }

    /// Touch the session to reset its idle TTL.
    pub fn touch(&self) {
        *self.last_active.lock().expect("last_active mutex poisoned") = Instant::now();
    }

    /// Append a serialized WS message to the replay buffer.
    ///
    /// Enforces two limits: a byte-based cap (`MAX_REPLAY_BUFFER_BYTES`, 4 MiB)
    /// and a secondary message-count cap (`MAX_REPLAY_BUFFER`, 4096). Messages
    /// that would exceed either limit are silently dropped.
    pub fn buffer_event(&self, json: String) {
        let msg_bytes = json.len();
        let mut bytes = self
            .replay_buffer_bytes
            .lock()
            .expect("replay_buffer_bytes mutex poisoned");
        let mut buf = self
            .replay_buffer
            .lock()
            .expect("replay_buffer mutex poisoned");
        if buf.len() < MAX_REPLAY_BUFFER && *bytes + msg_bytes <= MAX_REPLAY_BUFFER_BYTES {
            *bytes += msg_bytes;
            buf.push(json);
        }
    }

    /// Drain and return all buffered events, clearing the buffer and resetting
    /// the byte counter. Canonical implementation used by both reconnect replay
    /// and explicit session termination paths.
    pub fn drain_replay_buffer(&self) -> Vec<String> {
        let mut bytes = self
            .replay_buffer_bytes
            .lock()
            .expect("replay_buffer_bytes mutex poisoned");
        let mut buf = self
            .replay_buffer
            .lock()
            .expect("replay_buffer mutex poisoned");
        *bytes = 0;
        std::mem::take(&mut *buf)
    }

    /// Read and drain all buffered events for replay to a reconnecting client.
    ///
    /// Semantics: the first reconnect receives all buffered events (catch-up),
    /// then the buffer is cleared. Subsequent reconnects see an empty buffer
    /// unless new events were buffered in the interim. This prevents duplicate
    /// replays that would otherwise exhaust the replay cap with stale events.
    ///
    /// Delegates to `drain_replay_buffer()` -- both operations have identical
    /// drain-and-clear semantics.
    pub fn read_replay_buffer(&self) -> Vec<String> {
        self.drain_replay_buffer()
    }

    fn is_expired(&self) -> bool {
        let last = *self.last_active.lock().expect("last_active mutex poisoned");
        last.elapsed() > SESSION_TTL
    }
}

/// Global cache of live ACP adapter sessions.
///
/// Keyed by `agent_key` (e.g. `"Claude:mcp=12345"`), same key used by
/// `get_or_create_acp_connection` in `pulse_chat.rs`.
pub struct AcpSessionCache {
    sessions: DashMap<String, Arc<CachedSession>>,
    /// Maps ACP `session_id` → `agent_key` for reconnect lookups.
    session_id_index: DashMap<String, String>,
    reaper_started: std::sync::Once,
}

impl AcpSessionCache {
    fn new() -> Self {
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

    /// Evict expired sessions.
    async fn reap_expired(&self) {
        // Pass 1: clone keys + Arc refs (sync — releases DashMap shard locks immediately)
        let candidates: Vec<(String, Arc<CachedSession>)> = self
            .sessions
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        // Pass 2: check expiry — no DashMap locks held
        let mut to_remove = Vec::new();
        for (key, session) in &candidates {
            if session.is_expired() {
                to_remove.push(key.clone());
            }
        }
        for key in &to_remove {
            log::info!("[acp_cache] evicting expired session: {key}");
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
async fn reaper_loop() {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        SESSION_CACHE.reap_expired().await;
    }
}

#[cfg(test)]
mod tests {
    use super::super::persistent_conn::AcpConnectionHandle;
    use super::*;

    /// Create a test-only PermissionResponderMap.
    fn test_responder_map() -> PermissionResponderMap {
        Arc::new(DashMap::new())
    }

    /// Create a test-only AcpConnectionHandle wrapped in Arc.
    fn test_handle() -> Arc<AcpConnectionHandle> {
        Arc::new(AcpConnectionHandle::dummy())
    }

    #[test]
    fn cache_starts_empty() {
        let cache = AcpSessionCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn insert_and_get_round_trip() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        cache.insert("agent:1".into(), handle, responders);
        assert_eq!(cache.len(), 1);

        let session = cache.get("agent:1");
        assert!(session.is_some());
    }

    #[tokio::test]
    async fn insert_remove_get_returns_none() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        cache.insert("agent:2".into(), handle, responders);
        assert_eq!(cache.len(), 1);

        cache.remove("agent:2");
        assert_eq!(cache.len(), 0);
        assert!(cache.get("agent:2").is_none());
    }

    #[tokio::test]
    async fn session_id_index_lookup() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        cache.insert("agent:3".into(), handle, responders);
        cache.register_session_id("sess-abc".into(), "agent:3".into());

        let session = cache.get_by_session_id("sess-abc");
        assert!(session.is_some());

        // Unknown session_id returns None.
        assert!(cache.get_by_session_id("sess-unknown").is_none());
    }

    #[tokio::test]
    async fn remove_cleans_session_id_index() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        cache.insert("agent:4".into(), handle, responders);
        cache.register_session_id("sess-def".into(), "agent:4".into());

        cache.remove("agent:4");

        // session_id index entry should also be gone.
        assert!(cache.get_by_session_id("sess-def").is_none());
    }

    #[tokio::test]
    async fn buffer_event_and_drain_replay() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:5".into(), handle, responders);

        session.buffer_event(r#"{"type":"event1"}"#.into());
        session.buffer_event(r#"{"type":"event2"}"#.into());

        let events = session.drain_replay_buffer();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], r#"{"type":"event1"}"#);
        assert_eq!(events[1], r#"{"type":"event2"}"#);

        // Drain again — should be empty after first drain.
        let events = session.drain_replay_buffer();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn read_replay_buffer_drains_after_first_read() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:read_drain".into(), handle, responders);

        session.buffer_event(r#"{"type":"alpha"}"#.into());
        session.buffer_event(r#"{"type":"beta"}"#.into());

        // First read returns both buffered events.
        let first = session.read_replay_buffer();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], r#"{"type":"alpha"}"#);
        assert_eq!(first[1], r#"{"type":"beta"}"#);

        // Second read returns empty — buffer was drained on first read.
        let second = session.read_replay_buffer();
        assert!(
            second.is_empty(),
            "expected empty buffer after first read, got {}",
            second.len()
        );

        // drain_replay_buffer on an already-empty buffer is a no-op.
        let drained = session.drain_replay_buffer();
        assert!(drained.is_empty());

        // New events buffered after drain are returned on next read.
        session.buffer_event(r#"{"type":"gamma"}"#.into());
        let third = session.read_replay_buffer();
        assert_eq!(third.len(), 1);
        assert_eq!(third[0], r#"{"type":"gamma"}"#);
    }

    #[tokio::test]
    async fn replay_buffer_respects_max_capacity() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:6".into(), handle, responders);

        // Each message is small (~6 bytes), so the count cap (4096) hits first.
        for i in 0..MAX_REPLAY_BUFFER + 100 {
            session.buffer_event(format!("msg-{i}"));
        }

        let events = session.drain_replay_buffer();
        assert_eq!(events.len(), MAX_REPLAY_BUFFER);
    }

    #[tokio::test]
    async fn replay_buffer_respects_byte_limit() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:6b".into(), handle, responders);

        // Each message is 1024 bytes. With a 4 MiB byte cap, at most 4096 fit,
        // but here we push well beyond to confirm the byte cap stops buffering.
        let big_msg = "X".repeat(1024);
        let max_by_bytes = MAX_REPLAY_BUFFER_BYTES / big_msg.len();
        for _ in 0..max_by_bytes + 500 {
            session.buffer_event(big_msg.clone());
        }

        let events = session.drain_replay_buffer();
        let total_bytes: usize = events.iter().map(|e| e.len()).sum();
        assert!(
            total_bytes <= MAX_REPLAY_BUFFER_BYTES,
            "total bytes {total_bytes} exceeds limit {MAX_REPLAY_BUFFER_BYTES}"
        );
        assert_eq!(events.len(), max_by_bytes);
    }

    #[tokio::test]
    async fn replay_buffer_byte_counter_resets_on_drain() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:6c".into(), handle, responders);

        // Fill near the byte limit.
        let big_msg = "Y".repeat(1024);
        let max_by_bytes = MAX_REPLAY_BUFFER_BYTES / big_msg.len();
        for _ in 0..max_by_bytes {
            session.buffer_event(big_msg.clone());
        }

        // Drain resets the counter.
        let first = session.drain_replay_buffer();
        assert_eq!(first.len(), max_by_bytes);

        // Should be able to buffer again after drain.
        session.buffer_event(big_msg.clone());
        let second = session.drain_replay_buffer();
        assert_eq!(second.len(), 1);
    }

    #[tokio::test]
    async fn reap_expired_evicts_old_sessions() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:7".into(), handle, responders);

        // Manually backdate the last_active to far past the TTL.
        {
            let mut last = session.last_active.lock().expect("poisoned");
            *last = Instant::now() - SESSION_TTL - Duration::from_secs(60);
        }

        cache.reap_expired().await;

        assert!(cache.get("agent:7").is_none());
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn reap_expired_keeps_active_sessions() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        // Fresh session — should not be reaped.
        cache.insert("agent:8".into(), handle, responders);

        cache.reap_expired().await;

        assert!(cache.get("agent:8").is_some());
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn get_sync_does_not_touch() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:9".into(), handle, responders);

        // Backdate so it's almost expired.
        let almost_expired = Instant::now() - SESSION_TTL + Duration::from_secs(5);
        {
            let mut last = session.last_active.lock().expect("poisoned");
            *last = almost_expired;
        }

        // get_sync should NOT reset the TTL.
        let _ = cache.get_sync("agent:9");

        let last = *session.last_active.lock().expect("poisoned");
        // Should still be near the backdated time, not refreshed.
        assert!(last.elapsed() > Duration::from_secs(SESSION_TTL.as_secs() - 10));
    }
}
