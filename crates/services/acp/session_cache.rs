//! Global ACP session cache — adapters survive WebSocket disconnects.
//!
//! `SESSION_CACHE` is a process-wide singleton that holds `AcpConnectionHandle`
//! instances keyed by agent key. When a WebSocket disconnects, the adapter stays
//! alive in the cache. On reconnect, the client resumes the same adapter and
//! receives any events buffered during the disconnect.
//!
//! Idle sessions are reaped after `SESSION_TTL` (default 30 minutes).
//!
//! ## Global session count cap
//!
//! `AXON_ACP_MAX_SESSIONS` (default: 100, `0` = unlimited) limits the total
//! number of cached sessions. When the cache is at capacity and a new session
//! is inserted, the least-recently-used session (by `last_active`) is evicted.
//!
//! **Not the same as `AXON_ACP_MAX_CONCURRENT_SESSIONS`**, which is a semaphore
//! that gates how many ACP adapter processes may run *simultaneously*. This cap
//! is purely about how many idle sessions the cache may hold across the process.

mod cache;
mod entry;

pub use cache::AcpSessionCache;
pub use entry::CachedSession;

use std::time::Duration;

/// Default idle TTL before a cached session is evicted.
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// A turn in-flight longer than this threshold is considered hung. The reaper
/// evicts such sessions so the next request spawns a fresh adapter. Aligned
/// with the default per-turn timeout in `pulse_chat.rs` (`DEFAULT_TURN_TIMEOUT`).
const SESSION_HUNG_TURN_THRESHOLD: Duration = Duration::from_secs(5 * 60);

/// Hard cap on message count per session replay buffer. Secondary guard against
/// pathological cases with many tiny messages (primary limit is byte-based).
const MAX_REPLAY_BUFFER: usize = 4096;

/// Maximum cumulative byte size of the replay buffer per session (4 MiB).
/// Primary memory bound — a single `TurnResult` can be many KB, so counting
/// messages alone is insufficient.
const MAX_REPLAY_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// Global cap on cached ACP sessions. Read once from `AXON_ACP_MAX_SESSIONS`
/// at startup. `0` means unlimited. Default: 100.
///
/// When the cache reaches this limit and a new session is inserted, the
/// least-recently-used session (by `last_active`) is evicted.
pub(super) static MAX_SESSIONS: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
    std::env::var("AXON_ACP_MAX_SESSIONS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
});

/// Process-wide ACP session cache.
pub static SESSION_CACHE: std::sync::LazyLock<AcpSessionCache> =
    std::sync::LazyLock::new(AcpSessionCache::new);

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use dashmap::DashMap;

    use super::super::PermissionResponderMap;
    use super::super::persistent_conn::AcpConnectionHandle;
    use super::cache::AcpSessionCache;
    use super::{
        MAX_REPLAY_BUFFER, MAX_REPLAY_BUFFER_BYTES, SESSION_HUNG_TURN_THRESHOLD, SESSION_TTL,
    };

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
    async fn drain_replay_buffer_drains_after_first_call() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:read_drain".into(), handle, responders);

        session.buffer_event(r#"{"type":"alpha"}"#.into());
        session.buffer_event(r#"{"type":"beta"}"#.into());

        // First drain returns both buffered events.
        let first = session.drain_replay_buffer();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], r#"{"type":"alpha"}"#);
        assert_eq!(first[1], r#"{"type":"beta"}"#);

        // Second drain returns empty — buffer was drained on first call.
        let second = session.drain_replay_buffer();
        assert!(
            second.is_empty(),
            "expected empty buffer after first drain, got {}",
            second.len()
        );

        // New events buffered after drain are returned on next drain.
        session.buffer_event(r#"{"type":"gamma"}"#.into());
        let third = session.drain_replay_buffer();
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

    #[tokio::test]
    async fn reap_evicts_hung_turns() {
        let cache = AcpSessionCache::new();
        let handle = test_handle();
        let responders = test_responder_map();

        let session = cache.insert("agent:hung".into(), handle, responders);
        session.mark_turn_started();

        // Backdate the turn start to beyond the hung threshold.
        {
            let mut started = session.turn_in_flight_since.lock().expect("poisoned");
            *started = Some(Instant::now() - SESSION_HUNG_TURN_THRESHOLD - Duration::from_secs(10));
        }

        cache.reap_expired().await;

        assert!(cache.get("agent:hung").is_none());
    }

    // ── Global session cap tests ──────────────────────────────────────────────

    /// Helper: insert `n` sessions into `cache` with keys `"cap-agent:{i}"`.
    /// Returns the key of the first session inserted (useful as the "oldest").
    fn insert_n_sessions(cache: &AcpSessionCache, n: usize) -> String {
        for i in 0..n {
            let handle = test_handle();
            let responders = test_responder_map();
            cache.insert(format!("cap-agent:{i}"), handle, responders);
        }
        "cap-agent:0".to_owned()
    }

    /// Inserting sessions up to the cap does NOT trigger eviction.
    #[tokio::test]
    async fn cap_no_eviction_below_limit() {
        let cache = AcpSessionCache::new();
        // Insert exactly 3 sessions; cap is 5 — no eviction expected.
        insert_n_sessions(&cache, 3);
        cache.evict_if_over_cap(5);
        assert_eq!(cache.len(), 3);
    }

    /// When the cache exceeds cap, the LRU session is evicted.
    #[tokio::test]
    async fn cap_evicts_lru_when_over_cap() {
        let cache = AcpSessionCache::new();

        // Insert two sessions.
        let handle_a = test_handle();
        let handle_b = test_handle();
        let session_a = cache.insert("cap-a".into(), handle_a, test_responder_map());
        let _session_b = cache.insert("cap-b".into(), handle_b, test_responder_map());

        // Backdate session_a so it is the least recently used.
        {
            let mut last = session_a.last_active.lock().expect("poisoned");
            *last = Instant::now() - Duration::from_secs(600);
        }

        // Evict with a cap of 1 — must remove the oldest (cap-a).
        cache.evict_if_over_cap(1);

        assert_eq!(cache.len(), 1);
        assert!(
            cache.get("cap-a").is_none(),
            "LRU session should be evicted"
        );
        assert!(cache.get("cap-b").is_some(), "newer session should survive");
    }

    /// When the cache is at exactly the cap, no eviction occurs.
    #[tokio::test]
    async fn cap_no_eviction_at_exactly_cap() {
        let cache = AcpSessionCache::new();
        insert_n_sessions(&cache, 3);
        cache.evict_if_over_cap(3);
        assert_eq!(cache.len(), 3);
    }

    /// cap=0 means unlimited — evict_if_over_cap is skipped in insert().
    /// This test verifies that passing 0 directly does nothing.
    #[tokio::test]
    async fn cap_zero_means_unlimited() {
        let cache = AcpSessionCache::new();
        insert_n_sessions(&cache, 10);
        // Calling with cap=0 should not evict anything (caller guards cap>0).
        cache.evict_if_over_cap(0);
        assert_eq!(cache.len(), 10);
    }

    /// Evicting an LRU session also clears its session_id_index entry.
    #[tokio::test]
    async fn cap_eviction_clears_session_id_index() {
        let cache = AcpSessionCache::new();

        let handle_a = test_handle();
        let handle_b = test_handle();
        let session_a = cache.insert("idx-a".into(), handle_a, test_responder_map());
        let _session_b = cache.insert("idx-b".into(), handle_b, test_responder_map());

        // Register a session_id for the soon-to-be-evicted session.
        cache.register_session_id("sess-evict".into(), "idx-a".into());

        // Backdate session_a so it is the LRU.
        {
            let mut last = session_a.last_active.lock().expect("poisoned");
            *last = Instant::now() - Duration::from_secs(600);
        }

        cache.evict_if_over_cap(1);

        assert!(cache.get("idx-a").is_none());
        assert!(
            cache.get_by_session_id("sess-evict").is_none(),
            "session_id index entry should be cleaned up on eviction"
        );
    }
}
