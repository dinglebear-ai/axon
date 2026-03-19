use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_warn;
use std::error::Error;

use super::super::streaming::{ask_llm_non_streaming, ask_llm_streaming};

pub(crate) async fn ask_llm_answer(
    cfg: &Config,
    query: &str,
    context: &str,
) -> Result<(String, u128, bool), Box<dyn Error>> {
    let client = http_client()?;
    let llm_started = std::time::Instant::now();
    // Streaming to stdout is disabled: the ask command always collects the full
    // answer before returning it to the caller (CLI JSON output, MCP response,
    // or web UI). The stream_to_stdout=false flag tells the streaming path to
    // buffer internally rather than printing chunks.
    let stream_to_stdout = false;

    let (answer_opt, streamed_ok) = {
        let streamed = ask_llm_streaming(cfg, client, query, context, stream_to_stdout).await;
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
