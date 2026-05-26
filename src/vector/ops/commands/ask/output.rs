use crate::core::config::Config;
use crate::core::http::http_client;
use crate::core::logging::{log_info, log_warn};
use crate::services::llm_backend;
use std::error::Error;
use std::time::Instant;

use super::super::streaming::{ask_llm_non_streaming, ask_llm_streaming_ttft};

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

pub(crate) async fn ask_llm_answer(
    cfg: &Config,
    query: &str,
    context: &str,
) -> Result<AskLlmCompletion, Box<dyn Error>> {
    let client = http_client()?;
    let llm_started = Instant::now();
    // Keep stdout rendering centralized in the CLI command. The streaming path
    // still gives TTFT/fallback behavior, but token deltas printed here would be
    // followed by the CLI's final formatted answer.
    let stream_to_stdout = cfg.ask_stream && !cfg.json_output && !cfg.ask_explain;
    log_info(&format!(
        "ask llm request start backend={:?} model={} stream={} context_chars={} query_len={}",
        cfg.llm_backend,
        llm_backend::configured_model_from_config(cfg).unwrap_or_else(|| "<default>".to_string()),
        cfg.ask_stream,
        context.len(),
        query.len(),
    ));

    // The error type from streaming is `Box<dyn StdError>` (!Send). Collapse it
    // into Option<(String, Option<Instant>)> + Option<String> here so the !Send
    // error never crosses the await boundary that follows.
    let (streamed_ok, streamed_err): (Option<(String, Option<Instant>)>, Option<String>) = {
        let result = ask_llm_streaming_ttft(cfg, client, query, context, stream_to_stdout).await;
        match result {
            Ok(pair) => (Some(pair), None),
            Err(e) => (None, Some(e.to_string())),
        }
    };

    if let Some(err_msg) = streamed_err {
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

    match streamed_ok.expect("streamed_err handled above") {
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
