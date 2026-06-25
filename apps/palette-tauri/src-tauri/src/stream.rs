use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::axon_bridge::StreamClient;
use crate::{merged_settings, validate_saved_server_url};

/// Maximum allowed size (in bytes) for a single SSE line.
///
/// A rogue or misconfigured server could send an unbounded stream of bytes
/// without a newline, growing the in-memory `pending` buffer without limit.
/// This cap returns an error rather than allowing OOM growth.
const MAX_SSE_LINE_BYTES: usize = 1024 * 1024; // 1 MiB

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaletteStreamRequest {
    // Intentionally ignored — the real value is loaded from app settings
    #[serde(default, rename = "baseUrl")]
    _base_url: Option<String>,
    // Intentionally ignored — the real value is loaded from app settings
    #[serde(default, rename = "token")]
    _token: Option<String>,
    request_id: String,
    path: String,
    body: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum PaletteStreamEvent {
    Started {
        request_id: String,
        path: String,
    },
    Delta {
        request_id: String,
        text: String,
    },
    Activity {
        request_id: String,
        label: String,
        detail: Option<String>,
        kind: Option<String>,
    },
    Done {
        request_id: String,
        answer: Option<String>,
    },
    Error {
        request_id: String,
        message: String,
    },
}

#[tauri::command]
pub(crate) async fn axon_http_stream_request(
    app: AppHandle,
    window: tauri::Window,
    stream_client: tauri::State<'_, StreamClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    request: PaletteStreamRequest,
) -> Result<(), String> {
    if !matches!(request.path.as_str(), "/v1/ask/stream" | "/v1/chat/stream") {
        return Err("stream request path is not an allowed Axon API route".to_string());
    }
    let settings = merged_settings(&app)?;
    let base_url = validate_saved_server_url(&settings.server_url)?;
    let url = format!("{}{}", base_url.trim_end_matches('/'), request.path);
    window
        .emit(
            "palette://stream",
            PaletteStreamEvent::Started {
                request_id: request.request_id.clone(),
                path: request.path.clone(),
            },
        )
        .map_err(|err| err.to_string())?;

    let client = (*stream_client).client();
    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let body = &request.body;
    let make = |token: Option<&str>| {
        let mut b = client
            .post(&url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(body);
        if let Some(t) = token {
            b = b.bearer_auth(t).header("x-api-key", t);
        }
        b
    };
    let response =
        crate::oauth::send_with_reauth(&app, client, &base_url, static_token, &oauth_state, make)
            .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = crate::axon_bridge::read_limited_text_body(
            response,
            crate::axon_bridge::MAX_ARTIFACT_ERROR_MESSAGE_BYTES,
        )
        .await
        .unwrap_or_default();
        return Err(format!("stream request failed with HTTP {status}: {text}"));
    }

    let mut pending = Vec::new();
    let mut terminal = false;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        for line in drain_sse_lines(&mut pending, &chunk)? {
            terminal |= handle_palette_sse_line(&window, &request.request_id, &line)?;
        }
    }
    if !pending.is_empty() {
        let line = decode_sse_line(std::mem::take(&mut pending))?;
        if !line.trim().is_empty() {
            terminal |= handle_palette_sse_line(&window, &request.request_id, &line)?;
        }
    }
    if !terminal {
        let message = "stream ended before done".to_string();
        if let Err(err) = window.emit(
            "palette://stream",
            PaletteStreamEvent::Error {
                request_id: request.request_id,
                message: message.clone(),
            },
        ) {
            crate::diag::warn(&format!("failed to emit stream error event: {err}"));
        }
        return Err(message);
    }
    Ok(())
}

fn drain_sse_lines(pending: &mut Vec<u8>, chunk: &[u8]) -> Result<Vec<String>, String> {
    pending.extend_from_slice(chunk);
    if pending.len() > MAX_SSE_LINE_BYTES && !pending.contains(&b'\n') {
        return Err(format!(
            "SSE line exceeded {MAX_SSE_LINE_BYTES} bytes without a newline — \
             refusing to buffer further"
        ));
    }
    let mut lines = Vec::new();
    while let Some(pos) = pending.iter().position(|byte| *byte == b'\n') {
        if pos > MAX_SSE_LINE_BYTES {
            return Err(format!(
                "SSE line of {pos} bytes exceeds the {MAX_SSE_LINE_BYTES}-byte limit"
            ));
        }
        let raw: Vec<u8> = pending.drain(..=pos).collect();
        lines.push(decode_sse_line(raw)?);
    }
    Ok(lines)
}

fn decode_sse_line(mut raw: Vec<u8>) -> Result<String, String> {
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }
    if raw.last() == Some(&b'\r') {
        raw.pop();
    }
    String::from_utf8(raw).map_err(|err| format!("invalid UTF-8 in SSE stream: {err}"))
}

fn parse_sse_data_line(line: &str) -> Option<String> {
    line.strip_prefix("data:")
        .map(|value| value.trim().to_string())
}

fn handle_palette_sse_line(
    window: &tauri::Window,
    request_id: &str,
    line: &str,
) -> Result<bool, String> {
    let Some(data) = parse_sse_data_line(line) else {
        return Ok(false);
    };
    let value: serde_json::Value = serde_json::from_str(&data).map_err(|err| err.to_string())?;
    match value.get("type").and_then(|kind| kind.as_str()) {
        Some("delta") => {
            let text = match value.get("text").and_then(|t| t.as_str()) {
                Some(t) => t,
                None => {
                    crate::diag::warn(&format!(
                        "delta SSE event missing 'text' field — data: {data}"
                    ));
                    ""
                }
            };
            window
                .emit(
                    "palette://stream",
                    PaletteStreamEvent::Delta {
                        request_id: request_id.to_string(),
                        text: text.to_string(),
                    },
                )
                .map_err(|err| err.to_string())?;
            Ok(false)
        }
        Some("meta") => {
            let phase = value
                .get("phase")
                .and_then(|phase| phase.as_str())
                .unwrap_or("working");
            window
                .emit(
                    "palette://stream",
                    PaletteStreamEvent::Activity {
                        request_id: request_id.to_string(),
                        label: sentence_label(phase),
                        detail: None,
                        kind: Some("thinking".to_string()),
                    },
                )
                .map_err(|err| err.to_string())?;
            Ok(false)
        }
        Some("activity") => {
            let label = value
                .get("label")
                .and_then(|label| label.as_str())
                .unwrap_or("Working")
                .to_string();
            let detail = value
                .get("detail")
                .and_then(|detail| detail.as_str())
                .map(str::to_string);
            let kind = value
                .get("kind")
                .and_then(|kind| kind.as_str())
                .map(str::to_string);
            window
                .emit(
                    "palette://stream",
                    PaletteStreamEvent::Activity {
                        request_id: request_id.to_string(),
                        label,
                        detail,
                        kind,
                    },
                )
                .map_err(|err| err.to_string())?;
            Ok(false)
        }
        Some("done") => {
            let answer = done_answer_from_value(&value);
            window
                .emit(
                    "palette://stream",
                    PaletteStreamEvent::Done {
                        request_id: request_id.to_string(),
                        answer,
                    },
                )
                .map_err(|err| err.to_string())?;
            Ok(true)
        }
        Some("error") => {
            let message = value
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("stream error")
                .to_string();
            window
                .emit(
                    "palette://stream",
                    PaletteStreamEvent::Error {
                        request_id: request_id.to_string(),
                        message,
                    },
                )
                .map_err(|err| err.to_string())?;
            Ok(true)
        }
        Some(unknown) => {
            crate::diag::warn(&format!("unknown SSE event type '{unknown}' — ignoring"));
            Ok(false)
        }
        None => Ok(false),
    }
}

fn sentence_label(value: &str) -> String {
    let mut words = value
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return "Working".to_string();
    }
    let first = words.remove(0);
    let mut label = String::new();
    let mut chars = first.chars();
    if let Some(ch) = chars.next() {
        label.extend(ch.to_uppercase());
        label.push_str(chars.as_str());
    }
    for word in words {
        label.push(' ');
        label.push_str(word);
    }
    label
}

fn done_answer_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("answer")
        .and_then(|answer| answer.as_str())
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get("answer"))
                .and_then(|answer| answer.as_str())
        })
        .map(str::to_string)
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;
