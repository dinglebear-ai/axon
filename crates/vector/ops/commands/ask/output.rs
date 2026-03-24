use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_warn;
use crate::crates::services::acp_llm::WarmAcpSession;
use std::error::Error;
use std::io::IsTerminal as _;

use super::super::streaming::{ask_llm_non_streaming, ask_llm_streaming};

pub(crate) async fn ask_llm_answer(
    cfg: &Config,
    query: &str,
    context: &str,
    warm: Option<WarmAcpSession>,
) -> Result<(String, u128, bool), Box<dyn Error>> {
    let client = http_client()?;
    let llm_started = std::time::Instant::now();
    // Stream tokens to stdout only when writing to an interactive terminal and
    // not in JSON output mode. MCP (stdout = JSON-RPC pipe) and web callers
    // (no terminal) correctly get false here — no protocol corruption.
    let stream_to_stdout = !cfg.json_output && std::io::stdout().is_terminal();

    let (answer_opt, streamed_ok) = {
        let streamed = ask_llm_streaming(cfg, client, query, context, stream_to_stdout, warm).await;
        match streamed {
            Ok(ans) => (Some(ans), true),
            Err(e) => {
                log_warn(&format!(
                    "streaming failed, falling back to non-streaming: {e}"
                ));
                (None, false)
            }
        }
    };

    let answer = if let Some(ans) = answer_opt {
        ans
    } else {
        ask_llm_non_streaming(cfg, client, query, context).await?
    };

    Ok((answer, llm_started.elapsed().as_millis(), streamed_ok))
}
