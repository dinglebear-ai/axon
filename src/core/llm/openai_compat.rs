use crate::core::llm::{CompletionRequest, CompletionResponse, LlmBackendConfig, UsageSnapshot};
use crate::services::events::is_secret_like;
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::error::Error as StdError;
use std::sync::LazyLock;
use std::time::Duration;

#[cfg(test)]
#[path = "openai_compat_tests.rs"]
mod tests;

#[allow(non_upper_case_globals)]
static OpenAiCompatClients: LazyLock<dashmap::DashMap<u64, reqwest::Client>> =
    LazyLock::new(dashmap::DashMap::new);

pub fn openai_chat_completions_url(
    config: &LlmBackendConfig,
) -> Result<String, Box<dyn StdError + Send + Sync>> {
    let base = config
        .openai_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("AXON_OPENAI_BASE_URL is required when AXON_LLM_BACKEND=openai-compat")?;
    let trimmed = base.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        return Err(
            "AXON_OPENAI_BASE_URL must not include /chat/completions; use the API root such as http://127.0.0.1:8080/v1"
                .into(),
        );
    }
    Ok(format!("{trimmed}/chat/completions"))
}

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    let response = send_chat_completion(&req, false).await?;
    parse_chat_completion(response).await
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    mut on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let response = send_chat_completion(&req, true).await?;
    parse_sse_completion(response, &mut on_delta).await
}

async fn send_chat_completion(
    req: &CompletionRequest,
    stream: bool,
) -> Result<reqwest::Response, Box<dyn StdError + Send + Sync>> {
    let url = openai_chat_completions_url(&req.backend)?;
    let model = req
        .model
        .as_deref()
        .or(req.backend.openai_model.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(
            "AXON_SYNTHESIS_OPENAI_MODEL is required when AXON_LLM_BACKEND=openai-compat (legacy alias: AXON_OPENAI_MODEL)",
        )?;

    let mut messages = Vec::new();
    if let Some(system) = req
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.push(json!({ "role": "user", "content": req.user_prompt }));

    let body = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });
    let timeout_secs = req.backend.completion_timeout_secs.max(1);
    let client = OpenAiCompatClients
        .entry(timeout_secs)
        .or_try_insert_with(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
        })?
        .clone();
    let mut request = client.post(url).json(&body);
    if let Some(key) = req
        .backend
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(key);
    }
    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(format_openai_error(response).await.into());
    }
    Ok(response)
}

async fn format_openai_error(response: reqwest::Response) -> String {
    let status = response.status();
    match read_bounded_error_body(response).await {
        Ok(text) => {
            let safe_text = sanitize_openai_error_body(&text);
            if safe_text.trim().is_empty() {
                format!("OpenAI-compatible completion failed with HTTP {status}")
            } else {
                format!("OpenAI-compatible completion failed with HTTP {status}: {safe_text}")
            }
        }
        Err(err) => format!(
            "OpenAI-compatible completion failed with HTTP {status}; failed reading error body: {err}"
        ),
    }
}

async fn read_bounded_error_body(
    response: reqwest::Response,
) -> Result<String, Box<dyn StdError + Send + Sync>> {
    const READ_LIMIT: usize = 4096;
    let mut collected = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = READ_LIMIT.saturating_sub(collected.len());
        if remaining == 0 {
            break;
        }
        let take = chunk.len().min(remaining);
        collected.extend_from_slice(&chunk[..take]);
        if take < chunk.len() || collected.len() >= READ_LIMIT {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&collected).into_owned())
}

fn sanitize_openai_error_body(text: &str) -> String {
    const LIMIT: usize = 512;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let sanitized = sanitize_error_json(&value);
        let mut rendered = serde_json::to_string(&sanitized).unwrap_or_default();
        truncate_utf8_boundary(&mut rendered, LIMIT);
        return rendered;
    }
    let mut rendered = crate::core::redact::redact_secrets(trimmed);
    truncate_utf8_boundary(&mut rendered, LIMIT);
    rendered
}

fn truncate_utf8_boundary(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let end = value
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|idx| *idx <= max_bytes)
        .last()
        .unwrap_or(0);
    value.truncate(end);
    value.push_str("...[truncated]");
}

fn sanitize_error_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if is_sensitive_error_key(&lower) || is_request_echo_key(&lower) {
                        (
                            key.clone(),
                            serde_json::Value::String("[redacted]".to_string()),
                        )
                    } else {
                        (key.clone(), sanitize_error_json(value))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(sanitize_error_json).collect())
        }
        serde_json::Value::String(value) => {
            serde_json::Value::String(crate::core::redact::redact_secrets(value))
        }
        value => value.clone(),
    }
}

fn is_sensitive_error_key(lower_key: &str) -> bool {
    // Delegate to the shared `is_secret_like` helper (S-L1) — single source of
    // truth for both embed path validation and error-body redaction.
    is_secret_like(lower_key)
}

fn is_request_echo_key(lower_key: &str) -> bool {
    lower_key == "prompt"
        || lower_key == "input"
        || lower_key == "inputs"
        || lower_key == "messages"
        || lower_key == "request"
        || lower_key == "request_body"
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

async fn parse_chat_completion(
    response: reqwest::Response,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    let parsed: ChatCompletionResponse = response.json().await?;
    let text = parsed
        .choices
        .into_iter()
        .find_map(|choice| choice.message.and_then(|message| message.content))
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err("OpenAI-compatible completion returned no answer text".into());
    }
    let usage = parsed.usage.map(|usage| UsageSnapshot {
        prompt_tokens: usage.prompt_tokens.unwrap_or(0),
        completion_tokens: usage.completion_tokens.unwrap_or(0),
        total_tokens: usage.total_tokens.unwrap_or_else(|| {
            usage
                .prompt_tokens
                .unwrap_or(0)
                .saturating_add(usage.completion_tokens.unwrap_or(0))
        }),
    });
    Ok(CompletionResponse { text, usage })
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    finish_reason: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

async fn parse_sse_completion<F>(
    response: reqwest::Response,
    on_delta: &mut F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    if response.status() == StatusCode::NO_CONTENT {
        return Err("OpenAI-compatible streaming completion returned no content".into());
    }
    let mut text = String::new();
    // Buffer raw bytes; decode only on complete lines (Q-M8/B-M5).
    // The previous `from_utf8_lossy` per-chunk call corrupts multibyte chars
    // (e.g. `café`) when the network splits them across chunk boundaries.
    // `\n` (0x0A) is a single ASCII byte so splitting on it never bisects a
    // multibyte sequence — once we have a complete line, decoding is safe.
    let mut pending_bytes: Vec<u8> = Vec::new();
    let mut terminal = false;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        pending_bytes.extend_from_slice(&chunk?);
        while let Some(pos) = pending_bytes.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&pending_bytes[..pos]);
            terminal |= handle_sse_line(line.trim_end_matches('\r'), &mut text, on_delta)?;
            pending_bytes.drain(..=pos);
        }
    }
    if !pending_bytes.is_empty() {
        let line = String::from_utf8_lossy(&pending_bytes);
        let trimmed = line.trim_end_matches('\r');
        if !trimmed.trim().is_empty() {
            terminal |= handle_sse_line(trimmed, &mut text, on_delta)?;
        }
    }
    if text.trim().is_empty() {
        return Err("OpenAI-compatible streaming completion returned no token payload".into());
    }
    if !terminal {
        return Err("OpenAI-compatible streaming completion ended before terminal marker".into());
    }
    Ok(CompletionResponse { text, usage: None })
}

fn handle_sse_line<F>(
    line: &str,
    text: &mut String,
    on_delta: &mut F,
) -> Result<bool, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let Some(data) = line.strip_prefix("data:").map(str::trim) else {
        return Ok(false);
    };
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        return Ok(true);
    }
    let parsed: StreamChunk = serde_json::from_str(data)?;
    let mut terminal = false;
    for choice in parsed.choices {
        if choice
            .finish_reason
            .as_ref()
            .is_some_and(|reason| !reason.is_null())
        {
            terminal = true;
        }
        if let Some(delta) = choice.delta.and_then(|delta| delta.content)
            && !delta.is_empty()
        {
            text.push_str(&delta);
            on_delta(&delta)?;
        }
    }
    Ok(terminal)
}
