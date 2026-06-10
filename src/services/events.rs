use crate::core::logging::log_warn;
use serde::Serialize;
use std::error::Error as StdError;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Shared secret-name heuristic (S-L1).
///
/// Returns `true` when `lower_name` (already lowercased by caller) looks like
/// a secret key, credential file, or sensitive header/field name.  Single
/// source of truth for both embed path validation and error-body redaction.
pub fn is_secret_like(lower_name: &str) -> bool {
    // Private-key filenames
    if lower_name == "id_rsa"
        || lower_name == "id_dsa"
        || lower_name == "id_ecdsa"
        || lower_name == "id_ed25519"
    {
        return true;
    }
    // Extensions that commonly hold key material
    if lower_name.ends_with(".pem") || lower_name.ends_with(".key") {
        return true;
    }
    // Semantic keywords
    if lower_name.contains("secret")
        || lower_name.contains("credential")
        || lower_name.contains("password")
    {
        return true;
    }
    // Token / API key patterns
    if lower_name.contains("api_key")
        || lower_name.contains("apikey")
        || lower_name == "authorization"
        || lower_name == "proxy-authorization"
        || lower_name == "access_token"
        || lower_name == "refresh_token"
        || lower_name == "id_token"
        || lower_name.ends_with("_token")
        || lower_name.contains("token")
    {
        return true;
    }
    false
}

/// Build a streaming delta handler that forwards `SynthesisDelta` events over
/// an optional channel.  Drops silently after the first send error (logged once
/// per handler instance).
///
/// `label` is used in the one-time warning message to identify the caller
/// (e.g. `"research"`, `"summarize"`, `"ask"`).
pub fn synthesis_delta_handler(
    tx: Option<mpsc::Sender<ServiceEvent>>,
    label: &'static str,
) -> impl FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send {
    static WARNED_ONCE: AtomicBool = AtomicBool::new(false);
    move |delta| {
        if let Some(ref sender) = tx
            && let Err(e) = sender.try_send(ServiceEvent::SynthesisDelta {
                text: delta.to_string(),
            })
            && !WARNED_ONCE.swap(true, Ordering::Relaxed)
        {
            log_warn(&format!(
                "{label} synthesis_delta dropped (subsequent drops suppressed): {e}"
            ));
        }
        Ok(())
    }
}

/// Infallible variant of [`synthesis_delta_handler`] for callers that take
/// `impl FnMut(&str) + Send` (no `Result`).
pub fn synthesis_delta_handler_infallible(
    tx: Option<mpsc::Sender<ServiceEvent>>,
    label: &'static str,
) -> impl FnMut(&str) + Send {
    let mut inner = synthesis_delta_handler(tx, label);
    move |delta| {
        let _ = inner(delta);
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
