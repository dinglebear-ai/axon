// Scaffolding for streaming evaluate pipelines — not all functions are wired up yet.
#![expect(
    dead_code,
    reason = "scaffolding for streaming evaluate pipelines — not all functions are wired up yet"
)]

use crate::core::config::{Config, EvaluateResponsesMode};
use crate::core::logging::log_warn;
use std::error::Error;
use std::future::Future;
use std::io::{IsTerminal, Write as _};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::services::acp_llm::WarmAcpSession;

use super::super::streaming::{
    JudgeContext, TaggedToken, ask_llm_non_streaming, ask_llm_streaming, ask_llm_streaming_tagged,
    baseline_llm_non_streaming, baseline_llm_streaming, baseline_llm_streaming_tagged,
    judge_llm_non_streaming, judge_llm_streaming,
};
use super::SideBySideBuffer;
use super::display::{build_side_by_side_frame, emit_event, repaint_frame, terminal_width};

pub(super) const STREAM_WITH_CONTEXT: &str = "with_context";
pub(super) const STREAM_WITHOUT_CONTEXT: &str = "without_context";

pub(super) async fn run_rag_answer(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
    warm: Option<WarmAcpSession>,
) -> Result<(String, u128), Box<dyn Error>> {
    let started = Instant::now();
    let streaming = ask_llm_streaming(cfg, client, query, context, !cfg.json_output, warm).await;
    let answer = match streaming.map_err(|e| e.to_string()) {
        Ok(v) => v,
        Err(message) => {
            log_warn(&format!(
                "rag streaming failed, falling back to non-streaming: {message}"
            ));
            let fallback = ask_llm_non_streaming(cfg, client, query, context).await?;
            if !cfg.json_output {
                print!("{fallback}");
            }
            fallback
        }
    };
    Ok((answer, started.elapsed().as_millis()))
}

pub(super) async fn run_baseline_answer(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    warm: Option<WarmAcpSession>,
) -> Result<(String, u128), Box<dyn Error>> {
    let started = Instant::now();
    let streaming = baseline_llm_streaming(cfg, client, query, !cfg.json_output, warm).await;
    let answer = match streaming.map_err(|e| e.to_string()) {
        Ok(v) => v,
        Err(message) => {
            log_warn(&format!(
                "baseline streaming failed, falling back to non-streaming: {message}"
            ));
            let fallback = baseline_llm_non_streaming(cfg, client, query).await?;
            if !cfg.json_output {
                print!("{fallback}");
            }
            fallback
        }
    };
    Ok((answer, started.elapsed().as_millis()))
}

pub(super) async fn run_analysis(
    cfg: &Config,
    client: &reqwest::Client,
    judge_ctx: &JudgeContext<'_>,
    warm: Option<WarmAcpSession>,
) -> (String, u128) {
    let started = Instant::now();
    let print_tokens =
        !cfg.json_output && cfg.evaluate_responses_mode != EvaluateResponsesMode::Events;
    let answer = match judge_llm_streaming(cfg, client, judge_ctx, print_tokens, warm).await {
        Ok(v) => v,
        Err(e) => {
            log_warn(&format!(
                "judge streaming failed, falling back to non-streaming: {e}"
            ));
            match judge_llm_non_streaming(cfg, client, judge_ctx).await {
                Ok(fallback) => {
                    if !cfg.json_output {
                        print!("{fallback}");
                    }
                    fallback
                }
                Err(e2) => {
                    log_warn(&format!(
                        "evaluate: both streaming and non-streaming judge failed: {e2}"
                    ));
                    String::from(
                        "(judge unavailable — both streaming and non-streaming LLM calls failed)",
                    )
                }
            }
        }
    };
    (answer, started.elapsed().as_millis())
}

/// Build the two parallel answer futures and return the token receiver channel.
/// The sender is dropped here so the channel closes naturally when both futures complete.
#[allow(clippy::type_complexity)]
fn build_parallel_futures(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
) -> (
    impl Future<Output = Result<(String, u128), Box<dyn Error>>>,
    impl Future<Output = Result<(String, u128), Box<dyn Error>>>,
    mpsc::UnboundedReceiver<TaggedToken>,
) {
    let (tx, rx) = mpsc::unbounded_channel::<TaggedToken>();
    let rag_tx = tx.clone();
    let baseline_tx = tx.clone();
    drop(tx);

    let rag_cfg = cfg.clone();
    let baseline_cfg = cfg.clone();
    let rag_client = client.clone();
    let baseline_client = client.clone();
    let rag_query = query.to_string();
    let baseline_query = query.to_string();
    let rag_context = context.to_string();

    let rag_future = async move {
        let started = Instant::now();
        let answer = match ask_llm_streaming_tagged(
            &rag_cfg,
            &rag_client,
            &rag_query,
            &rag_context,
            STREAM_WITH_CONTEXT,
            &rag_tx,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                log_warn(&format!(
                    "rag parallel streaming failed, falling back to non-streaming: {e}"
                ));
                let fallback =
                    ask_llm_non_streaming(&rag_cfg, &rag_client, &rag_query, &rag_context).await?;
                let _ = rag_tx.send(TaggedToken {
                    stream: STREAM_WITH_CONTEXT,
                    delta: fallback.clone(),
                });
                fallback
            }
        };
        Ok::<(String, u128), Box<dyn Error>>((answer, started.elapsed().as_millis()))
    };

    let baseline_future = async move {
        let started = Instant::now();
        let answer = match baseline_llm_streaming_tagged(
            &baseline_cfg,
            &baseline_client,
            &baseline_query,
            STREAM_WITHOUT_CONTEXT,
            &baseline_tx,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                log_warn(&format!(
                    "baseline parallel streaming failed, falling back to non-streaming: {e}"
                ));
                let fallback =
                    baseline_llm_non_streaming(&baseline_cfg, &baseline_client, &baseline_query)
                        .await?;
                let _ = baseline_tx.send(TaggedToken {
                    stream: STREAM_WITHOUT_CONTEXT,
                    delta: fallback.clone(),
                });
                fallback
            }
        };
        Ok::<(String, u128), Box<dyn Error>>((answer, started.elapsed().as_millis()))
    };

    (rag_future, baseline_future, rx)
}

/// Emit a single inline token, switching label when the active stream changes.
fn handle_token_inline(
    evt: &TaggedToken,
    active: &mut Option<&'static str>,
) -> Result<(), Box<dyn Error>> {
    let label = match evt.stream {
        STREAM_WITH_CONTEXT => "[RAG]",
        STREAM_WITHOUT_CONTEXT => "[BASE]",
        _ => "[STREAM]",
    };
    if *active != Some(evt.stream) {
        if active.is_some() {
            println!();
        }
        print!("  {label} ");
        std::io::stdout().flush()?;
        *active = Some(evt.stream);
    }
    print!("{}", evt.delta);
    std::io::stdout().flush()?;
    Ok(())
}

/// Emit a single side-by-side token, repainting the frame on each delta.
fn handle_token_side_by_side(
    evt: &TaggedToken,
    side_by_side: &mut SideBySideBuffer,
    rendered_lines: &mut usize,
    active: &mut Option<&'static str>,
    side_by_side_supported: bool,
) -> Result<(), Box<dyn Error>> {
    if side_by_side_supported {
        side_by_side.push(evt.stream, &evt.delta);
        let frame = build_side_by_side_frame(
            terminal_width(),
            &side_by_side.with_context,
            &side_by_side.without_context,
        );
        *rendered_lines = repaint_frame(&frame, *rendered_lines)?;
    } else {
        handle_token_inline(evt, active)?;
    }
    Ok(())
}

pub(super) async fn run_parallel_answers_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
    mode: EvaluateResponsesMode,
) -> Result<((String, u128), (String, u128)), Box<dyn Error>> {
    let (rag_future, baseline_future, mut rx) = build_parallel_futures(cfg, client, query, context);

    tokio::pin!(rag_future);
    tokio::pin!(baseline_future);

    let mut active: Option<&'static str> = None;
    let mut side_by_side = SideBySideBuffer::new();
    let mut rendered_lines = 0usize;
    let mut rag_result: Option<(String, u128)> = None;
    let mut baseline_result: Option<(String, u128)> = None;
    let side_by_side_supported = std::io::stdout().is_terminal();

    loop {
        tokio::select! {
            evt = rx.recv() => {
                match evt {
                    Some(evt) => {
                        match mode {
                            EvaluateResponsesMode::Inline => {
                                handle_token_inline(&evt, &mut active)?;
                            }
                            EvaluateResponsesMode::SideBySide => {
                                handle_token_side_by_side(
                                    &evt,
                                    &mut side_by_side,
                                    &mut rendered_lines,
                                    &mut active,
                                    side_by_side_supported,
                                )?;
                            }
                            EvaluateResponsesMode::Events => {
                                emit_event(&serde_json::json!({
                                    "type": "token",
                                    "stream": evt.stream,
                                    "delta": evt.delta,
                                }))?;
                            }
                        }
                    }
                    None => {
                        if rag_result.is_some() && baseline_result.is_some() {
                            break;
                        }
                    }
                }
            }
            res = &mut rag_future, if rag_result.is_none() => {
                let done = res?;
                if mode == EvaluateResponsesMode::Events {
                    emit_event(&serde_json::json!({
                        "type": "stream_done",
                        "stream": STREAM_WITH_CONTEXT,
                        "elapsed_ms": done.1,
                        "chars": done.0.len(),
                    }))?;
                }
                rag_result = Some(done);
                if rag_result.is_some() && baseline_result.is_some() && rx.is_closed() {
                    break;
                }
            }
            res = &mut baseline_future, if baseline_result.is_none() => {
                let done = res?;
                if mode == EvaluateResponsesMode::Events {
                    emit_event(&serde_json::json!({
                        "type": "stream_done",
                        "stream": STREAM_WITHOUT_CONTEXT,
                        "elapsed_ms": done.1,
                        "chars": done.0.len(),
                    }))?;
                }
                baseline_result = Some(done);
                if rag_result.is_some() && baseline_result.is_some() && rx.is_closed() {
                    break;
                }
            }
        }
    }

    if mode != EvaluateResponsesMode::Events {
        println!();
    }
    Ok((
        rag_result.ok_or("missing rag answer from parallel streaming")?,
        baseline_result.ok_or("missing baseline answer from parallel streaming")?,
    ))
}

#[cfg(test)]
mod tests;
