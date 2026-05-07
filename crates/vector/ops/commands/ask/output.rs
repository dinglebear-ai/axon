use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_warn;
use crate::crates::services::acp_llm::WarmAcpSession;
use std::error::Error;
use std::io::IsTerminal as _;
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
    warm: Option<WarmAcpSession>,
) -> Result<AskLlmCompletion, Box<dyn Error>> {
    let client = http_client()?;
    let llm_started = Instant::now();
    // Stream tokens to stdout only when writing to an interactive terminal and
    // not in JSON output mode. MCP (stdout = JSON-RPC pipe) and web callers
    // (no terminal) correctly get false here — no protocol corruption.
    let stream_to_stdout = !cfg.json_output && std::io::stdout().is_terminal();

    // The error type from streaming is `Box<dyn StdError>` (!Send). Collapse it
    // into Option<(String, Option<Instant>)> + Option<String> here so the !Send
    // error never crosses the await boundary that follows.
    let (streamed_ok, streamed_err): (Option<(String, Option<Instant>)>, Option<String>) = {
        let result =
            ask_llm_streaming_ttft(cfg, client, query, context, stream_to_stdout, warm).await;
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
        return Ok(AskLlmCompletion::Fallback {
            answer,
            llm_total_ms: llm_started.elapsed().as_millis(),
        });
    }

    match streamed_ok.expect("streamed_err handled above") {
        (answer, Some(ttft_at)) => Ok(AskLlmCompletion::Streamed {
            answer,
            ttft_at,
            llm_total_ms: llm_started.elapsed().as_millis(),
        }),
        (answer, None) => Ok(AskLlmCompletion::Fallback {
            answer,
            llm_total_ms: llm_started.elapsed().as_millis(),
        }),
    }
}
