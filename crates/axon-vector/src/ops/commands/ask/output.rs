use axon_core::config::Config;
use axon_core::http::http_client;
use axon_core::logging::{log_info, log_warn};
use axon_llm as llm;
use std::error::Error;
use std::time::Instant;

use super::super::streaming::{
    ask_llm_non_streaming, ask_llm_streaming_ttft, ask_llm_streaming_ttft_with_callback,
};

/// Outcome of one ask LLM round-trip — sum type so callers cannot reach for a
/// `ttft_at` field that exists only on the streaming path.
pub(crate) enum AskLlmCompletion {
    Streamed {
        answer: String,
        ttft_at: Instant,
        llm_total_ms: u128,
    },
    Fallback {
        answer: String,
        llm_total_ms: u128,
    },
}

pub(crate) fn append_ask_delta(answer: &mut String, delta: &str) {
    answer.push_str(delta);
}

pub(crate) async fn ask_llm_answer(
    cfg: &Config,
    query: &str,
    context: &str,
) -> Result<AskLlmCompletion, Box<dyn Error>> {
    ask_llm_answer_impl(
        cfg,
        query,
        context,
        cfg.ask_stream && !cfg.json_output && !cfg.ask_explain,
        Option::<fn(&str)>::None,
    )
    .await
}

pub(crate) async fn ask_llm_answer_with_deltas<F>(
    cfg: &Config,
    query: &str,
    context: &str,
    on_delta: F,
) -> Result<AskLlmCompletion, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    ask_llm_answer_impl(cfg, query, context, false, Some(on_delta)).await
}

async fn ask_llm_answer_impl<F>(
    cfg: &Config,
    query: &str,
    context: &str,
    stream_to_stdout: bool,
    mut on_delta: Option<F>,
) -> Result<AskLlmCompletion, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    let client = http_client()?;
    let llm_started = Instant::now();
    log_info(&format!(
        "ask llm request start backend={:?} model={} stream={} context_chars={} query_len={}",
        cfg.llm_backend,
        llm::configured_model_from_config(cfg).unwrap_or_else(|| "<default>".to_string()),
        cfg.ask_stream,
        context.len(),
        query.len(),
    ));

    // The error type from streaming is `Box<dyn StdError>` (!Send). Collapse it
    // into Option<(String, Option<Instant>)> + Option<String> here so the !Send
    // error never crosses the await boundary that follows.
    let streamed = {
        let result = if let Some(mut callback) = on_delta.take() {
            let mut callback_answer = String::new();
            ask_llm_streaming_ttft_with_callback(
                cfg,
                client,
                query,
                context,
                stream_to_stdout,
                move |delta| {
                    append_ask_delta(&mut callback_answer, delta);
                    callback(delta);
                },
            )
            .await
        } else {
            ask_llm_streaming_ttft(cfg, client, query, context, stream_to_stdout).await
        };
        result.map_err(|e| e.to_string())
    };

    let streamed = match streamed {
        Ok(streamed) => streamed,
        Err(err_msg) => {
            log_warn(&format!(
                "ask: streaming failed, falling back to non-streaming: {err_msg}"
            ));
            let answer = ask_llm_non_streaming(cfg, client, query, context).await?;
            log_info(&format!(
                "ask llm fallback complete answer_chars={} elapsed_ms={}",
                answer.len(),
                llm_started.elapsed().as_millis(),
            ));
            return Ok(AskLlmCompletion::Fallback {
                answer,
                llm_total_ms: llm_started.elapsed().as_millis(),
            });
        }
    };

    match streamed {
        (answer, Some(ttft_at)) => {
            let llm_total_ms = llm_started.elapsed().as_millis();
            log_info(&format!(
                "ask llm streaming complete answer_chars={} elapsed_ms={llm_total_ms}",
                answer.len(),
            ));
            Ok(AskLlmCompletion::Streamed {
                answer,
                ttft_at,
                llm_total_ms,
            })
        }
        (answer, None) => {
            let llm_total_ms = llm_started.elapsed().as_millis();
            log_info(&format!(
                "ask llm non-streaming complete answer_chars={} elapsed_ms={llm_total_ms}",
                answer.len(),
            ));
            Ok(AskLlmCompletion::Fallback {
                answer,
                llm_total_ms,
            })
        }
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
