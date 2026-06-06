use crate::services::llm_backend::{
    CompletionRequest, CompletionResponse, LlmBackendConfig, UsageSnapshot,
};
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::error::Error as StdError;

#[cfg(test)]
#[path = "openai_compat_tests.rs"]
mod tests;

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
        .ok_or("AXON_OPENAI_MODEL is required when AXON_LLM_BACKEND=openai-compat")?;

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
    let client = reqwest::Client::builder()
        .timeout(req.backend.completion_timeout())
        .build()?;
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
    let text = response.text().await.unwrap_or_default();
    if text.trim().is_empty() {
        format!("OpenAI-compatible completion failed with HTTP {status}")
    } else {
        format!("OpenAI-compatible completion failed with HTTP {status}: {text}")
    }
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
            usage.prompt_tokens.unwrap_or(0) + usage.completion_tokens.unwrap_or(0)
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
    let mut pending = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = pending.find('\n') {
            let line = pending[..pos].trim_end_matches('\r').to_string();
            pending.drain(..=pos);
            handle_sse_line(&line, &mut text, on_delta)?;
        }
    }
    if !pending.trim().is_empty() {
        handle_sse_line(pending.trim_end_matches('\r'), &mut text, on_delta)?;
    }
    if text.trim().is_empty() {
        return Err("OpenAI-compatible streaming completion returned no token payload".into());
    }
    Ok(CompletionResponse { text, usage: None })
}

fn handle_sse_line<F>(
    line: &str,
    text: &mut String,
    on_delta: &mut F,
) -> Result<(), Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let Some(data) = line.strip_prefix("data:").map(str::trim) else {
        return Ok(());
    };
    if data.is_empty() || data == "[DONE]" {
        return Ok(());
    }
    let parsed: StreamChunk = serde_json::from_str(data)?;
    for choice in parsed.choices {
        if let Some(delta) = choice.delta.and_then(|delta| delta.content)
            && !delta.is_empty()
        {
            text.push_str(&delta);
            on_delta(&delta)?;
        }
    }
    Ok(())
}
