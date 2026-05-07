use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_warn;
use crate::crates::services::acp_llm::WarmAcpSession;
use std::error::Error;
use std::io::IsTerminal as _;
use std::time::Instant;

use super::super::streaming::{ask_llm_non_streaming, ask_llm_streaming_ttft};

/// Outcome of one ask LLM round-trip with sub-stage timings (bd axon_rust-nm9).
/// `ttft_first_token_at` is the absolute `Instant` of the first non-empty
/// streaming delta; `None` if streaming failed and the non-streaming fallback
/// was used. The caller computes the TTFT delta relative to its request-start
/// (typically captured at CLI entry).
pub(crate) struct AskLlmAnswer {
    pub answer: String,
    pub llm_total_ms: u128,
    pub streamed_ok: bool,
    pub ttft_first_token_at: Option<Instant>,
}

pub(crate) async fn ask_llm_answer(
    cfg: &Config,
    query: &str,
    context: &str,
    warm: Option<WarmAcpSession>,
) -> Result<AskLlmAnswer, Box<dyn Error>> {
    let client = http_client()?;
    let llm_started = Instant::now();
    // Stream tokens to stdout only when writing to an interactive terminal and
    // not in JSON output mode. MCP (stdout = JSON-RPC pipe) and web callers
    // (no terminal) correctly get false here — no protocol corruption.
    let stream_to_stdout = !cfg.json_output && std::io::stdout().is_terminal();

    let (answer_opt, ttft_at, streamed_ok) = {
        let streamed =
            ask_llm_streaming_ttft(cfg, client, query, context, stream_to_stdout, warm).await;
        match streamed {
            Ok((ans, ttft)) => (Some(ans), ttft, true),
            Err(e) => {
                log_warn(&format!(
                    "streaming failed, falling back to non-streaming: {e}"
                ));
                (None, None, false)
            }
        }
    };

    let answer = if let Some(ans) = answer_opt {
        ans
    } else {
        ask_llm_non_streaming(cfg, client, query, context).await?
    };

    Ok(AskLlmAnswer {
        answer,
        llm_total_ms: llm_started.elapsed().as_millis(),
        streamed_ok,
        ttft_first_token_at: ttft_at,
    })
}
