use crate::events::{EventKind, EventRecorder, RecordedEvent, sanitize_value};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::STDERR_CHUNK_BYTES;

pub(super) fn spawn_stderr_reader(
    mut stderr: tokio::process::ChildStderr,
    events: broadcast::Sender<RecordedEvent>,
    recorder: EventRecorder,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut chunk = [0_u8; STDERR_CHUNK_BYTES];
        loop {
            match stderr.read(&mut chunk).await {
                Ok(0) => break,
                Ok(read) => {
                    let detail = redact_stderr(&String::from_utf8_lossy(&chunk[..read]));
                    if !detail.is_empty() {
                        emit_protocol_failure(
                            &events,
                            &recorder,
                            format!("Codex app-server stderr: {detail}"),
                        );
                    }
                }
                Err(error) => {
                    emit_protocol_failure(
                        &events,
                        &recorder,
                        format!("failed to read Codex app-server stderr: {error}"),
                    );
                    break;
                }
            }
        }
    })
}

pub(super) fn redact_stderr(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "cookie",
        "credential",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return "[REDACTED]".to_string();
    }
    match sanitize_value(Value::String(trimmed.to_string())) {
        Value::String(value) => value,
        _ => "[REDACTED]".to_string(),
    }
}

pub(super) fn emit_protocol_failure(
    events: &broadcast::Sender<RecordedEvent>,
    recorder: &EventRecorder,
    detail: String,
) {
    let _ = events.send(recorder.record(EventKind::ProtocolFailure { detail }));
}
