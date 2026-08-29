//! Bounded, cursor-addressable, secret-safe control event history.

use crate::api::{contains_sensitive_url, is_sensitive_identifier};
use crate::protocol::RuntimeEpoch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use utoipa::ToSchema;

pub const MAX_EVENT_HISTORY: usize = 512;
const MAX_EVENT_STRING: usize = 4096;
const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventCursor {
    pub boot_id: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        request_id: u64,
        method: String,
        params: Value,
    },
    ProtocolFailure {
        detail: String,
    },
    Exited,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RecordedEvent {
    pub cursor: EventCursor,
    pub event: EventKind,
}

#[derive(Debug, Clone)]
pub struct EventRecorder {
    boot_id: u64,
    next: Arc<AtomicU64>,
    history: Arc<Mutex<VecDeque<RecordedEvent>>>,
}

impl EventRecorder {
    pub fn new(epoch: RuntimeEpoch) -> Self {
        Self {
            boot_id: epoch.0,
            next: Arc::new(AtomicU64::new(1)),
            history: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_EVENT_HISTORY))),
        }
    }

    pub fn record(&self, event: EventKind) -> RecordedEvent {
        let event = sanitize_event(event);
        let recorded = RecordedEvent {
            cursor: EventCursor {
                boot_id: self.boot_id,
                sequence: self.next.fetch_add(1, Ordering::Relaxed),
            },
            event,
        };
        let mut history = self
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if history.len() == MAX_EVENT_HISTORY {
            history.pop_front();
        }
        history.push_back(recorded.clone());
        recorded
    }

    pub fn after(
        &self,
        cursor: Option<EventCursor>,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, String> {
        let history = self
            .history
            .lock()
            .map_err(|_| "Codex event history lock poisoned".to_string())?;
        if let Some(cursor) = cursor {
            if cursor.boot_id != self.boot_id {
                return Err(
                    "Codex event cursor belongs to a previous runtime; refresh the snapshot"
                        .to_string(),
                );
            }
            if let Some(first) = history.front()
                && cursor.sequence.saturating_add(1) < first.cursor.sequence
            {
                return Err("Codex event cursor gap; refresh the snapshot".to_string());
            }
        }
        let limit = limit.min(100);
        if cursor.is_none() {
            let start = history.len().saturating_sub(limit);
            return Ok(history.iter().skip(start).cloned().collect());
        }
        let sequence = cursor.map_or(0, |value| value.sequence);
        Ok(history
            .iter()
            .filter(|event| event.cursor.sequence > sequence)
            .take(limit)
            .cloned()
            .collect())
    }
}

fn sanitize_event(event: EventKind) -> EventKind {
    match event {
        EventKind::Notification { method, params } => EventKind::Notification {
            method,
            params: sanitize_event_payload(params),
        },
        EventKind::ServerRequest {
            request_id,
            method,
            params,
        } => EventKind::ServerRequest {
            request_id,
            method,
            params: sanitize_event_payload(params),
        },
        EventKind::ProtocolFailure { detail } => EventKind::ProtocolFailure {
            detail: truncate(detail),
        },
        EventKind::Exited => EventKind::Exited,
    }
}

fn sanitize_event_payload(value: Value) -> Value {
    let sanitized = sanitize_value(value);
    if serde_json::to_vec(&sanitized).is_ok_and(|bytes| bytes.len() <= MAX_EVENT_PAYLOAD_BYTES) {
        sanitized
    } else {
        serde_json::json!({"truncated": true, "reason": "event payload exceeded 64 KiB"})
    }
}

pub fn sanitize_value(value: Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .take(100)
                .map(|(key, value)| {
                    let secret = is_sensitive_identifier(&key);
                    (
                        key,
                        if secret {
                            Value::String("[REDACTED]".to_string())
                        } else {
                            sanitize_value(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.into_iter().take(100).map(sanitize_value).collect())
        }
        Value::String(value) if contains_sensitive_url(&value) => {
            Value::String("[REDACTED]".to_string())
        }
        Value::String(value) => Value::String(truncate(value.replace(['\r', '\0'], ""))),
        other => other,
    }
}

fn truncate(mut value: String) -> String {
    if value.len() <= MAX_EVENT_STRING {
        return value;
    }
    let mut boundary = MAX_EVENT_STRING;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value.push('…');
    value
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
