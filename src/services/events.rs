use serde::Serialize;
use tokio::sync::mpsc;

/// The write operation for an `EditorWrite` event.
///
/// Serializes to `"replace"` or `"append"` on the wire, matching the
/// TypeScript `'replace' | 'append'` union and the Zod schema in
/// `web event consumers`.
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
        if s.eq_ignore_ascii_case("warn") || s.eq_ignore_ascii_case("warning") {
            Self::Warn
        } else if s.eq_ignore_ascii_case("error") {
            Self::Error
        } else {
            Self::Info
        }
    }
}

impl From<String> for LogLevel {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ServiceEvent {
    Log {
        level: LogLevel,
        message: String,
    },
    /// Emitted after a turn completes when the agent's response contained
    /// one or more `<axon:editor>` blocks.  Each block becomes one event.
    EditorWrite {
        content: String,
        operation: EditorOperation,
    },
    /// Streaming token delta from research synthesis (one event per chunk).
    ///
    /// Emitted by [`crate::services::search::research`] as the LLM
    /// writes its synthesis response.  CLI handlers stream these to stderr
    /// inline; web handlers forward them as `{"type":"synthesis_delta"}`.
    SynthesisDelta {
        text: String,
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

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
