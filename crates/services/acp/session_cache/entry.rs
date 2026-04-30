//! `CachedSession` — single live ACP adapter entry with replay buffer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::PermissionResponderMap;
use super::super::persistent_conn::AcpConnectionHandle;
use super::{MAX_REPLAY_BUFFER, MAX_REPLAY_BUFFER_BYTES, SESSION_TTL};

/// Replay buffer state: messages and cumulative byte size under a single lock.
pub(super) struct ReplayBufferState {
    pub(super) messages: Vec<String>,
    pub(super) total_bytes: usize,
}

/// A cached ACP adapter session.
pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    pub(super) last_active: std::sync::Mutex<Instant>,
    /// Serialized WS JSON messages buffered while no client is connected,
    /// with cumulative byte tracking under one lock.
    pub(super) replay_buffer: std::sync::Mutex<ReplayBufferState>,
    /// When the current in-flight turn started, if any. `None` means no turn
    /// is currently running. Used by the reaper and `get_or_create_acp_connection`
    /// to detect adapters that are stuck (turn in-flight longer than threshold).
    pub(super) turn_in_flight_since: std::sync::Mutex<Option<Instant>>,
    /// When the last turn completed successfully. Used for diagnostics.
    pub(super) last_turn_completed_at: std::sync::Mutex<Option<Instant>>,
}

impl CachedSession {
    pub(super) fn new(
        handle: Arc<AcpConnectionHandle>,
        permission_responders: PermissionResponderMap,
    ) -> Self {
        Self {
            handle,
            permission_responders,
            last_active: std::sync::Mutex::new(Instant::now()),
            replay_buffer: std::sync::Mutex::new(ReplayBufferState {
                messages: Vec::new(),
                total_bytes: 0,
            }),
            turn_in_flight_since: std::sync::Mutex::new(None),
            last_turn_completed_at: std::sync::Mutex::new(None),
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
        let mut buf = self
            .replay_buffer
            .lock()
            .expect("replay_buffer mutex poisoned");
        if buf.messages.len() < MAX_REPLAY_BUFFER
            && buf.total_bytes + msg_bytes <= MAX_REPLAY_BUFFER_BYTES
        {
            buf.total_bytes += msg_bytes;
            buf.messages.push(json);
        } else {
            tracing::warn!(
                buffered_msgs = buf.messages.len(),
                buffered_bytes = buf.total_bytes,
                dropped_msg_bytes = msg_bytes,
                max_msgs = MAX_REPLAY_BUFFER,
                max_bytes = MAX_REPLAY_BUFFER_BYTES,
                "acp: replay buffer cap reached — dropping event (reconnecting client will see truncated replay)"
            );
        }
    }

    /// Drain and return all buffered events, clearing the buffer and resetting
    /// the byte counter. Used by reconnect replay and session termination paths.
    pub fn drain_replay_buffer(&self) -> Vec<String> {
        let mut buf = self
            .replay_buffer
            .lock()
            .expect("replay_buffer mutex poisoned");
        buf.total_bytes = 0;
        std::mem::take(&mut buf.messages)
    }

    pub(super) fn is_expired(&self) -> bool {
        let last = *self.last_active.lock().expect("last_active mutex poisoned");
        last.elapsed() > SESSION_TTL
    }

    /// Record that a turn has started on this session.
    pub fn mark_turn_started(&self) {
        *self
            .turn_in_flight_since
            .lock()
            .expect("turn_in_flight_since mutex poisoned") = Some(Instant::now());
    }

    /// Record that the current turn has completed.
    pub fn mark_turn_completed(&self) {
        *self
            .turn_in_flight_since
            .lock()
            .expect("turn_in_flight_since mutex poisoned") = None;
        *self
            .last_turn_completed_at
            .lock()
            .expect("last_turn_completed_at mutex poisoned") = Some(Instant::now());
    }

    /// Returns `true` if a turn has been in-flight longer than `threshold`,
    /// indicating the adapter is likely hung.
    pub fn is_turn_hung(&self, threshold: Duration) -> bool {
        self.turn_in_flight_since
            .lock()
            .expect("turn_in_flight_since mutex poisoned")
            .is_some_and(|started| started.elapsed() > threshold)
    }
}
