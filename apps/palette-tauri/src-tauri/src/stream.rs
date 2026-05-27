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
    path: String,
    body: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PaletteStreamEvent {
    Started { path: String },
    Delta { text: String },
    Done { answer: Option<String> },
    Error { message: String },
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

    let mut pending = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = pending.find('\n') {
            let line = pending[..pos].trim_end_matches('\r').to_string();
            pending.drain(..=pos);
            handle_palette_sse_line(&window, &line)?;
        }
    }
    if !pending.trim().is_empty() {
        handle_palette_sse_line(&window, pending.trim_end_matches('\r'))?;
    }
    Ok(())
}

fn parse_sse_data_line(line: &str) -> Option<String> {
    line.strip_prefix("data:")
        .map(|value| value.trim().to_string())
}

fn handle_palette_sse_line(window: &tauri::Window, line: &str) -> Result<(), String> {
    let Some(data) = parse_sse_data_line(line) else {
        return Ok(());
    };
    let value: serde_json::Value = serde_json::from_str(&data).map_err(|err| err.to_string())?;
    match value.get("type").and_then(|kind| kind.as_str()) {
        Some("delta") => {
            let text = value
                .get("text")
                .and_then(|text| text.as_str())
                .unwrap_or_default();
            window.emit(
                "palette://stream",
                PaletteStreamEvent::Delta {
                    text: text.to_string(),
                },
            )
        }
        Some("done") => {
            let answer = value
                .get("answer")
                .and_then(|answer| answer.as_str())
                .map(str::to_string);
            window.emit("palette://stream", PaletteStreamEvent::Done { answer })
        }
        Some("error") => {
            let message = value
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("stream error")
                .to_string();
            window.emit("palette://stream", PaletteStreamEvent::Error { message })
        }
        _ => Ok(()),
    }
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod stream_tests {
    use super::parse_sse_data_line as parse_data_line;

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
}
