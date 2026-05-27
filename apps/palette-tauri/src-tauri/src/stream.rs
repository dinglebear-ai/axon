use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use super::normalize_server_url;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaletteStreamRequest {
    base_url: String,
    token: Option<String>,
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
    window: tauri::Window,
    request: PaletteStreamRequest,
) -> Result<(), String> {
    let base_url = normalize_server_url(&request.base_url);
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

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("Axon Palette/4.5")
        .build()
        .map_err(|err| err.to_string())?;
    let mut builder = client
        .post(url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&request.body);
    if let Some(token) = request
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
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
        let _ = window.emit(
            "palette://stream",
            PaletteStreamEvent::Error {
                request_id: request.request_id,
                message: message.clone(),
            },
        );
        return Err(message);
    }
    Ok(())
}

fn drain_sse_lines(pending: &mut Vec<u8>, chunk: &[u8]) -> Result<Vec<String>, String> {
    pending.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(pos) = pending.iter().position(|byte| *byte == b'\n') {
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
            let text = value
                .get("text")
                .and_then(|text| text.as_str())
                .unwrap_or_default();
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
        Some("done") => {
            let answer = value
                .get("answer")
                .and_then(|answer| answer.as_str())
                .map(str::to_string);
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
        _ => Ok(false),
    }
}

#[cfg(test)]
mod stream_tests {
    use super::{drain_sse_lines, parse_sse_data_line as parse_data_line};

    #[test]
    fn parse_sse_data_line() {
        assert_eq!(
            parse_data_line("data: {\"type\":\"delta\",\"text\":\"hi\"}"),
            Some("{\"type\":\"delta\",\"text\":\"hi\"}".to_string())
        );
    }

    #[test]
    fn ignores_non_data_sse_line() {
        assert_eq!(parse_data_line("event: delta"), None);
    }

    #[test]
    fn buffers_split_utf8_until_complete_line() {
        let mut pending = Vec::new();
        let snowman = "data: {\"type\":\"delta\",\"text\":\"☃\"}\n".as_bytes();
        assert!(
            drain_sse_lines(&mut pending, &snowman[..snowman.len() - 2])
                .unwrap()
                .is_empty()
        );

        let lines = drain_sse_lines(&mut pending, &snowman[snowman.len() - 2..]).unwrap();

        assert_eq!(lines, vec!["data: {\"type\":\"delta\",\"text\":\"☃\"}"]);
        assert!(pending.is_empty());
    }
}
