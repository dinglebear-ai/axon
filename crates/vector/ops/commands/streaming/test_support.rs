use super::*;
use crate::crates::services::acp_llm::AcpCompletionRunner;

fn extract_sse_token(data: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(data).ok()?;
    value["choices"][0]["delta"]["content"]
        .as_str()
        .or_else(|| value["choices"][0]["message"]["content"].as_str())
        .or_else(|| value["choices"][0]["text"].as_str())
        .map(str::to_string)
}

pub(crate) fn process_sse_line(
    line: &str,
    answer: &mut String,
    print_tokens: bool,
    saw_stream_payload: &mut bool,
    tagged: Option<(&UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<bool, Box<dyn Error>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with("data: ") {
        return Ok(false);
    }
    let data = trimmed.trim_start_matches("data: ").trim();
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        return Ok(true);
    }

    if let Some(token) = extract_sse_token(data) {
        *saw_stream_payload = true;
        answer.push_str(&token);
        if let Some((tx, stream)) = tagged {
            let _ = tx.send(TaggedToken {
                stream,
                delta: token.clone(),
            });
        }
        if print_tokens {
            print!("{token}");
            std::io::stdout().flush()?;
        }
    }
    Ok(false)
}

async fn run_acp_streaming_completion_with_runner<R>(
    runner: &R,
    req: AcpCompletionRequest,
    print_tokens: bool,
    tagged: Option<(UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<String, Box<dyn Error>>
where
    R: AcpCompletionRunner + ?Sized,
{
    let mut answer = String::new();
    let mut saw_stream_payload = false;
    let mut first_sources_pos: Option<usize> = None;
    let mut sources_search_from = 0usize;
    let mut repeat_guard_triggered = false;
    let response = acp_llm::complete_streaming_with_runner(runner, req, |delta| {
        if repeat_guard_triggered {
            return Ok(());
        }
        let _ = process_stream_delta(
            delta,
            &mut answer,
            print_tokens,
            &mut saw_stream_payload,
            tagged.as_ref(),
        )?;
        let scan_from = sources_search_from.saturating_sub(10);
        if let Some(second_pos) =
            check_sources_repetition(&answer, scan_from, &mut first_sources_pos)
        {
            answer.truncate(second_pos);
            repeat_guard_triggered = true;
        }
        sources_search_from = answer.len().saturating_sub(15);
        Ok(())
    })
    .await?;

    finalize_stream_answer(answer, saw_stream_payload, response.text)
}

async fn run_acp_text_completion_with_runner<R>(
    runner: &R,
    req: AcpCompletionRequest,
) -> AnyResult<String>
where
    R: AcpCompletionRunner + ?Sized,
{
    let response = acp_llm::complete_text_with_runner(runner, req)
        .await
        .map_err(|err| anyhow!(err.to_string()))?;
    Ok(response.text)
}

pub(crate) async fn ask_llm_streaming_with_runner<R>(
    runner: &R,
    cfg: &Config,
    query: &str,
    context: &str,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_streaming_completion_with_runner(
        runner,
        ask_completion_request(cfg, query, context, true),
        print_tokens,
        None,
    )
    .await
}

pub(crate) async fn ask_llm_streaming_tagged_with_runner<R>(
    runner: &R,
    cfg: &Config,
    query: &str,
    context: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_streaming_completion_with_runner(
        runner,
        ask_completion_request(cfg, query, context, true),
        false,
        Some((tx.clone(), stream)),
    )
    .await
}

pub(crate) async fn ask_llm_non_streaming_with_runner<R>(
    runner: &R,
    cfg: &Config,
    query: &str,
    context: &str,
) -> AnyResult<String>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_text_completion_with_runner(runner, ask_completion_request(cfg, query, context, false))
        .await
}

pub(crate) async fn baseline_llm_streaming_tagged_with_runner<R>(
    runner: &R,
    cfg: &Config,
    query: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_streaming_completion_with_runner(
        runner,
        baseline_completion_request(cfg, query, true),
        false,
        Some((tx.clone(), stream)),
    )
    .await
}

pub(crate) async fn baseline_llm_non_streaming_with_runner<R>(
    runner: &R,
    cfg: &Config,
    query: &str,
) -> AnyResult<String>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_text_completion_with_runner(runner, baseline_completion_request(cfg, query, false))
        .await
}

pub(crate) async fn judge_llm_non_streaming_with_runner<R>(
    runner: &R,
    cfg: &Config,
    ctx: &JudgeContext<'_>,
) -> AnyResult<String>
where
    R: AcpCompletionRunner + ?Sized,
{
    run_acp_text_completion_with_runner(runner, judge_completion_request(cfg, ctx, false)).await
}
