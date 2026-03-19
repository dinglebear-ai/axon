use crate::crates::core::config::Config;
use crate::crates::services::acp_llm::{self, AcpCompletionRequest};
use anyhow::{Result as AnyResult, anyhow};
use std::error::Error;
use std::io::Write;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) const ASK_RAG_SYSTEM_PROMPT: &str = r###"You are a source-grounded technical assistant.

You may answer ONLY from the provided retrieved context. Do not use unstated prior knowledge.

STEP 1 — RELEVANCE CHECK
- First decide whether the retrieved context is directly relevant to the user's question.
- Ignore keyword-only overlap; require clear topical alignment.

STEP 2 — OUTPUT POLICY

IF RELEVANT CONTEXT EXISTS:
1. Provide a concise answer grounded in the retrieved context.
2. Every material claim must include inline citations like [S1] or [S2][S4].
3. If the context is partially complete, include a brief "Gaps:" note describing what is missing.
4. End with a single "## Sources" section listing each cited source exactly once.

IF RELEVANT CONTEXT DOES NOT EXIST:
- State briefly that the indexed sources are insufficient for this question.
- Provide 1-3 concrete suggestions for what to index next (specific docs/pages/topics).
- Do not provide an uncited answer.
- Do not include a "from training knowledge" section."###;

const BASELINE_SYSTEM_PROMPT: &str = "You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them.";

/// Build a POST request to the OpenAI-compatible chat completions endpoint with
/// optional bearer auth. Retained for legacy command paths outside ask/evaluate.
pub(super) fn build_openai_chat_request(
    client: &reqwest::Client,
    cfg: &Config,
) -> reqwest::RequestBuilder {
    let mut req = client.post(format!(
        "{}/chat/completions",
        cfg.openai_base_url.trim_end_matches('/')
    ));
    if !cfg.openai_api_key.trim().is_empty() {
        req = req.bearer_auth(&cfg.openai_api_key);
    }
    req
}

/// Context for LLM judge comparison between RAG and baseline answers.
pub(crate) struct JudgeContext<'a> {
    pub query: &'a str,
    pub rag_answer: &'a str,
    pub baseline_answer: &'a str,
    pub reference_chunks: &'a str,
    pub rag_sources_list: &'a str,
    pub ref_quality_note: &'a str,
    pub rag_elapsed_ms: u128,
    pub baseline_elapsed_ms: u128,
    pub source_count: usize,
    pub context_chars: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // reason: used by streaming evaluate pipeline — wire up before release
pub(crate) struct TaggedToken {
    pub stream: &'static str,
    pub delta: String,
}

fn judge_system_prompt() -> &'static str {
    "You are an expert evaluator with access to authoritative reference material.\n\
Compare two AI responses to the same question.\n\
\n\
IMPORTANT INSTRUCTIONS:\n\
- Do NOT score higher simply because an answer is longer or more technical. Concise and accurate beats verbose and wandering.\n\
- First, enumerate the key factual claims in each answer. Then verify each claim against the Reference Material using [R#] citations.\n\
- If reference chunks contain version numbers or dates, note whether the baseline answer may be out of date relative to the indexed material.\n\
\n\
Produce your analysis in this EXACT format:\n\
\n\
## Accuracy        RAG: X/5 | Baseline: X/5\n\
[Reasoning with [R#] citations for specific claims. Note any factual errors or omissions.]\n\
\n\
## Relevance       RAG: X/5 | Baseline: X/5\n\
[Did each answer address what was actually asked?]\n\
\n\
## Completeness    RAG: X/5 | Baseline: X/5\n\
[Did each answer cover the important details?]\n\
\n\
## Specificity     RAG: X/5 | Baseline: X/5\n\
[Did each answer give concrete, actionable information?]\n\
\n\
## Timing\n\
[Was the RAG latency overhead justified by the quality improvement?]\n\
\n\
## Did RAG Add Value?\n\
YES/NO — [Did the indexed knowledge base provide information the LLM could not have had from training alone? Be specific.]\n\
\n\
## Verdict\n\
[1-2 sentences: which response is better overall and why?]"
}

fn judge_user_msg(ctx: &JudgeContext<'_>) -> String {
    format!(
        "Question: {query}\n\n\
## RAG Answer (WITH context — {source_count} sources, {context_chars} chars, {rag_ms}ms)\n\
Sources the RAG answer was built from:\n{rag_sources_list}\n\n\
{rag_answer}\n\n\
## Baseline Answer (WITHOUT context, {baseline_ms}ms)\n\
{baseline_answer}\n\n\
## Reference Material (independent retrieval for accuracy grounding)\n\
{ref_quality_note}\
{reference_chunks}\n\n\
Analyze and compare the two responses following the format in your instructions.",
        query = ctx.query,
        source_count = ctx.source_count,
        context_chars = ctx.context_chars,
        rag_ms = ctx.rag_elapsed_ms,
        rag_sources_list = ctx.rag_sources_list,
        rag_answer = ctx.rag_answer,
        baseline_ms = ctx.baseline_elapsed_ms,
        baseline_answer = ctx.baseline_answer,
        ref_quality_note = ctx.ref_quality_note,
        reference_chunks = ctx.reference_chunks,
    )
}

/// Scan `answer` (from `search_from` onwards) for a second `\n## Sources` occurrence.
/// Returns the byte index of the second occurrence if found, so the caller can truncate there.
/// `first_sources_pos` tracks where the first one was seen (None = not yet).
fn check_sources_repetition(
    answer: &str,
    search_from: usize,
    first_sources_pos: &mut Option<usize>,
) -> Option<usize> {
    let haystack = answer[search_from..].to_ascii_lowercase();
    let needle = "\n## sources";
    if let Some(rel) = haystack.find(needle) {
        let abs = search_from + rel;
        match *first_sources_pos {
            None => {
                *first_sources_pos = Some(abs);
            }
            Some(_) => {
                return Some(abs);
            }
        }
    }
    None
}

fn process_stream_delta(
    delta: &str,
    answer: &mut String,
    print_tokens: bool,
    saw_stream_payload: &mut bool,
    tagged: Option<&(UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<String, Box<dyn Error>> {
    if delta.is_empty() {
        return Ok(String::new());
    }

    *saw_stream_payload = true;
    answer.push_str(delta);
    if let Some((tx, stream)) = tagged {
        let _ = tx.send(TaggedToken {
            stream,
            delta: delta.to_string(),
        });
    }
    if print_tokens {
        print!("{delta}");
        std::io::stdout().flush()?;
    }
    Ok(delta.to_string())
}

fn finalize_stream_answer(
    answer: String,
    saw_stream_payload: bool,
    fallback_text: String,
) -> Result<String, Box<dyn Error>> {
    if saw_stream_payload && !answer.trim().is_empty() {
        return Ok(answer);
    }
    if !fallback_text.trim().is_empty() {
        return Ok(fallback_text);
    }
    Err("streaming response returned no token payload".into())
}

fn ask_completion_request(
    cfg: &Config,
    query: &str,
    context: &str,
    stream: bool,
) -> AcpCompletionRequest {
    let req = AcpCompletionRequest::new(format!("Question: {query}\n\nContext:\n{context}"))
        .system_prompt(ASK_RAG_SYSTEM_PROMPT)
        .stream(stream);
    apply_optional_model(req, &cfg.openai_model)
}

fn baseline_completion_request(cfg: &Config, query: &str, stream: bool) -> AcpCompletionRequest {
    let req = AcpCompletionRequest::new(query)
        .system_prompt(BASELINE_SYSTEM_PROMPT)
        .stream(stream);
    apply_optional_model(req, &cfg.openai_model)
}

fn judge_completion_request(
    cfg: &Config,
    ctx: &JudgeContext<'_>,
    stream: bool,
) -> AcpCompletionRequest {
    let req = AcpCompletionRequest::new(judge_user_msg(ctx))
        .system_prompt(judge_system_prompt())
        .stream(stream);
    apply_optional_model(req, &cfg.openai_model)
}

fn apply_optional_model(req: AcpCompletionRequest, model: &str) -> AcpCompletionRequest {
    if model.trim().is_empty() {
        req
    } else {
        req.model(model.to_string())
    }
}

async fn run_acp_streaming_completion(
    cfg: &Config,
    req: AcpCompletionRequest,
    print_tokens: bool,
    tagged: Option<(UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<String, Box<dyn Error>> {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || -> AnyResult<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!(err.to_string()))?;
        runtime.block_on(async move {
            let mut answer = String::new();
            let mut saw_stream_payload = false;
            let mut first_sources_pos: Option<usize> = None;
            let mut sources_search_from = 0usize;
            let mut repeat_guard_triggered = false;
            let response = acp_llm::complete_streaming(&cfg, req, |delta| {
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
            .await
            .map_err(|err| anyhow!(err.to_string()))?;

            finalize_stream_answer(answer, saw_stream_payload, response.text)
                .map_err(|err| anyhow!(err.to_string()))
        })
    })
    .await
    .map_err(|err| -> Box<dyn Error> { Box::new(err) })?
    .map_err(Into::into)
}

async fn run_acp_text_completion(cfg: &Config, req: AcpCompletionRequest) -> AnyResult<String> {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || -> AnyResult<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!(err.to_string()))?;
        runtime.block_on(async move {
            let response = acp_llm::complete_text(&cfg, req)
                .await
                .map_err(|err| anyhow!(err.to_string()))?;
            Ok(response.text)
        })
    })
    .await
    .map_err(|err| anyhow!(err.to_string()))?
}

pub(crate) async fn ask_llm_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
    context: &str,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    run_acp_streaming_completion(
        cfg,
        ask_completion_request(cfg, query, context, true),
        print_tokens,
        None,
    )
    .await
}

#[allow(dead_code)] // reason: used by streaming evaluate pipeline — wire up before release
pub(crate) async fn ask_llm_streaming_tagged(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
    context: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>> {
    run_acp_streaming_completion(
        cfg,
        ask_completion_request(cfg, query, context, true),
        false,
        Some((tx.clone(), stream)),
    )
    .await
}

pub(crate) async fn ask_llm_non_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
    context: &str,
) -> AnyResult<String> {
    run_acp_text_completion(cfg, ask_completion_request(cfg, query, context, false)).await
}

pub(crate) async fn baseline_llm_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    run_acp_streaming_completion(
        cfg,
        baseline_completion_request(cfg, query, true),
        print_tokens,
        None,
    )
    .await
}

#[allow(dead_code)] // reason: used by streaming evaluate pipeline — wire up before release
pub(crate) async fn baseline_llm_streaming_tagged(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>> {
    run_acp_streaming_completion(
        cfg,
        baseline_completion_request(cfg, query, true),
        false,
        Some((tx.clone(), stream)),
    )
    .await
}

pub(crate) async fn baseline_llm_non_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    query: &str,
) -> Result<String, Box<dyn Error>> {
    run_acp_text_completion(cfg, baseline_completion_request(cfg, query, false))
        .await
        .map_err(Into::into)
}

pub(crate) async fn judge_llm_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    ctx: &JudgeContext<'_>,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    run_acp_streaming_completion(
        cfg,
        judge_completion_request(cfg, ctx, true),
        print_tokens,
        None,
    )
    .await
}

pub(crate) async fn judge_llm_non_streaming(
    cfg: &Config,
    _client: &reqwest::Client,
    ctx: &JudgeContext<'_>,
) -> Result<String, Box<dyn Error>> {
    run_acp_text_completion(cfg, judge_completion_request(cfg, ctx, false))
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::{
    ask_llm_non_streaming_with_runner, ask_llm_streaming_tagged_with_runner,
    ask_llm_streaming_with_runner, baseline_llm_non_streaming_with_runner,
    baseline_llm_streaming_tagged_with_runner, judge_llm_non_streaming_with_runner,
    process_sse_line,
};
#[cfg(test)]
mod tests;
