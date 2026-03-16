use crate::crates::services::types::AcpBridgeEvent;
use serde::Serialize;
use tokio::sync::mpsc;

/// The write operation for an `EditorWrite` event.
///
/// Serializes to `"replace"` or `"append"` on the wire, matching the
/// TypeScript `'replace' | 'append'` union and the Zod schema in
/// `use-axon-acp.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorOperation {
    Replace,
    Append,
}

impl std::fmt::Display for EditorOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replace => write!(f, "replace"),
            Self::Append => write!(f, "append"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl From<&str> for LogLevel {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

impl From<String> for LogLevel {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEvent {
    Log {
        level: LogLevel,
        message: String,
    },
    AcpBridge {
        event: AcpBridgeEvent,
    },
    /// Emitted after a turn completes when the agent's response contained
    /// one or more `<axon:editor>` blocks.  Each block becomes one event.
    EditorWrite {
        content: String,
        operation: EditorOperation,
    },
}

pub async fn emit(tx: &Option<mpsc::Sender<ServiceEvent>>, event: ServiceEvent) {
    if let Some(sender) = tx {
        let _ = sender.send(event).await;
    }
}

/// Fire-and-forget variant of [`emit`] that never blocks the caller.
///
/// Uses `try_send` under the hood — if the channel is full the event is
/// silently dropped.  Use this in hot paths (stderr readers, streaming
/// notification handlers) where blocking on a full channel would stall an
/// unrelated subsystem (e.g. draining adapter stderr).
pub fn emit_nonblocking(tx: &Option<mpsc::Sender<ServiceEvent>>, event: ServiceEvent) {
    if let Some(sender) = tx {
        let _ = sender.try_send(event);
    }
}

/// Emit with a timeout — blocks up to `timeout` waiting for channel capacity,
/// then drops the event if the deadline expires.  Returns `true` if sent,
/// `false` on timeout or missing sender.
pub async fn emit_with_timeout(
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    event: ServiceEvent,
    timeout: std::time::Duration,
) -> bool {
    if let Some(sender) = tx {
        tokio::time::timeout(timeout, sender.send(event))
            .await
            .is_ok()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── LogLevel::from(&str) ──────────────────────────────────────────────────

    #[test]
    fn log_level_from_str_warn_variants() {
        assert_eq!(LogLevel::from("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::from("warning"), LogLevel::Warn);
    }

    #[test]
    fn log_level_from_str_error() {
        assert_eq!(LogLevel::from("error"), LogLevel::Error);
    }

    #[test]
    fn log_level_from_str_info() {
        assert_eq!(LogLevel::from("info"), LogLevel::Info);
    }

    /// Anything not explicitly matched ("debug", "trace") falls through to the
    /// `_` arm which returns `Info` — the default variant.
    /// Matching is case-insensitive, so uppercase variants are now handled.
    #[test]
    fn log_level_from_str_unknown_defaults_to_info() {
        assert_eq!(LogLevel::from("debug"), LogLevel::Info);
        assert_eq!(LogLevel::from("trace"), LogLevel::Info);
        // Case-insensitive: "WARN" now maps to Warn, not Info.
        assert_eq!(LogLevel::from("WARN"), LogLevel::Warn);
        assert_eq!(LogLevel::from("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from("WARNING"), LogLevel::Warn);
    }

    #[test]
    fn log_level_from_str_empty_defaults() {
        assert_eq!(LogLevel::from(""), LogLevel::Info);
    }

    // ── Display round-trips ───────────────────────────────────────────────────

    #[test]
    fn log_level_display_round_trips() {
        assert_eq!(format!("{}", LogLevel::Info), "info");
        assert_eq!(format!("{}", LogLevel::Warn), "warn");
        assert_eq!(format!("{}", LogLevel::Error), "error");
    }

    // ── emit() ────────────────────────────────────────────────────────────────

    fn make_log_event(msg: &str) -> ServiceEvent {
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg.to_string(),
        }
    }

    #[tokio::test]
    async fn emit_delivers_event_when_channel_has_room() {
        let (tx, mut rx) = mpsc::channel::<ServiceEvent>(4);
        let sender_opt = Some(tx);
        emit(&sender_opt, make_log_event("hello")).await;
        let received = rx.try_recv().expect("event should have been delivered");
        assert_eq!(received, make_log_event("hello"));
    }

    #[tokio::test]
    async fn emit_with_none_sender_does_not_panic() {
        // Passing None must not panic; the event is silently discarded.
        emit(&None, make_log_event("ignored")).await;
    }

    #[tokio::test]
    async fn emit_blocks_on_full_channel_and_delivers_after_drain() {
        let (tx, mut rx) = mpsc::channel::<ServiceEvent>(1);
        let sender_opt = Some(tx);
        // Fill the channel
        emit(&sender_opt, make_log_event("first")).await;
        // Spawn a task to send to a full channel — will block until receiver drains
        let sender_clone = sender_opt.clone();
        let send_task = tokio::spawn(async move {
            emit(&sender_clone, make_log_event("second")).await;
        });
        // Drain the channel
        let _ = rx.recv().await;
        // Now the send_task should complete
        send_task.await.unwrap();
        // Second event must be delivered (not dropped)
        let msg = rx.recv().await;
        assert!(
            msg.is_some(),
            "second event must be delivered after drain, not dropped"
        );
    }
}
