use crate::crates::core::config::Config;
use anyhow::Result as AnyResult;
use futures_util::StreamExt;
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

/// Build a POST request to the OpenAI-compatible chat completions endpoint with
/// optional bearer auth. Callers chain `.json(...)` to attach the request body.
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

fn extract_sse_token(data: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(data).ok()?;
    value["choices"][0]["delta"]["content"]
        .as_str()
        .or_else(|| value["choices"][0]["message"]["content"].as_str())
        .or_else(|| value["choices"][0]["text"].as_str())
        .map(str::to_string)
}

fn process_sse_line(
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

async fn run_sse_stream(
    req: reqwest::RequestBuilder,
    print_tokens: bool,
    tagged: Option<(&UnboundedSender<TaggedToken>, &'static str)>,
) -> Result<String, Box<dyn Error>> {
    let response = req.send().await?;
    if !response.status().is_success() {
        return Err(format!("streaming request failed with status {}", response.status()).into());
    }

    let mut stream = response.bytes_stream();
    let mut answer = String::new();
    let mut pending = String::new();
    let mut saw_stream_payload = false;
    // Repetition guard: tracks position of first \n## Sources so we can detect a second.
    let mut first_sources_pos: Option<usize> = None;
    let mut sources_search_from = 0usize;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        pending.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline_idx) = pending.find('\n') {
            let mut line = pending[..newline_idx].to_string();
            pending.drain(..=newline_idx);
            if line.ends_with('\r') {
                let _ = line.pop();
            }
            let len_before = answer.len();
            let done = process_sse_line(
                &line,
                &mut answer,
                print_tokens,
                &mut saw_stream_payload,
                tagged,
            )?;
            if done {
                return Ok(answer);
            }
            // Only scan newly appended content for the repetition pattern.
            if answer.len() > len_before {
                let scan_from = sources_search_from.saturating_sub(10); // small overlap for split tokens
                if let Some(second_pos) =
                    check_sources_repetition(&answer, scan_from, &mut first_sources_pos)
                {
                    // Second ## Sources found — truncate and stop streaming.
                    answer.truncate(second_pos);
                    return Ok(answer);
                }
                sources_search_from = answer.len().saturating_sub(15);
            }
        }
    }

    if !pending.trim().is_empty() {
        let _ = process_sse_line(
            &pending,
            &mut answer,
            print_tokens,
            &mut saw_stream_payload,
            tagged,
        )?;
    }

    if saw_stream_payload && !answer.trim().is_empty() {
        return Ok(answer);
    }

    Err("streaming response returned no token payload".into())
}

pub(crate) async fn ask_llm_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": ASK_RAG_SYSTEM_PROMPT},
            {"role": "user", "content": format!("Question: {}\n\nContext:\n{}", query, context)}
        ],
        "temperature": 0.1,
        "stream": true
    }));

    run_sse_stream(req, print_tokens, None).await
}

#[allow(dead_code)] // reason: used by streaming evaluate pipeline — wire up before release
pub(crate) async fn ask_llm_streaming_tagged(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": ASK_RAG_SYSTEM_PROMPT},
            {"role": "user", "content": format!("Question: {}\n\nContext:\n{}", query, context)}
        ],
        "temperature": 0.1,
        "stream": true
    }));

    run_sse_stream(req, false, Some((tx, stream))).await
}

pub(crate) async fn ask_llm_non_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
) -> AnyResult<String> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": ASK_RAG_SYSTEM_PROMPT},
            {"role": "user", "content": format!("Question: {}\n\nContext:\n{}", query, context)}
        ],
        "temperature": 0.1
    }));

    let response = req.send().await?.error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no answer)")
        .to_string())
}

pub(crate) async fn baseline_llm_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": "You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them."},
            {"role": "user", "content": query}
        ],
        "temperature": 0.1,
        "stream": true
    }));

    run_sse_stream(req, print_tokens, None).await
}

#[allow(dead_code)] // reason: used by streaming evaluate pipeline — wire up before release
pub(crate) async fn baseline_llm_streaming_tagged(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    stream: &'static str,
    tx: &UnboundedSender<TaggedToken>,
) -> Result<String, Box<dyn Error>> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": "You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them."},
            {"role": "user", "content": query}
        ],
        "temperature": 0.1,
        "stream": true
    }));

    run_sse_stream(req, false, Some((tx, stream))).await
}

pub(crate) async fn baseline_llm_non_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
) -> Result<String, Box<dyn Error>> {
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": "You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them."},
            {"role": "user", "content": query}
        ],
        "temperature": 0.1
    }));

    let response = req.send().await?.error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no answer)")
        .to_string())
}

pub(crate) async fn judge_llm_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    ctx: &JudgeContext<'_>,
    print_tokens: bool,
) -> Result<String, Box<dyn Error>> {
    let user_msg = judge_user_msg(ctx);
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": judge_system_prompt()},
            {"role": "user", "content": user_msg}
        ],
        "temperature": 0.3,
        "stream": true
    }));
    run_sse_stream(req, print_tokens, None).await
}

pub(crate) async fn judge_llm_non_streaming(
    cfg: &Config,
    client: &reqwest::Client,
    ctx: &JudgeContext<'_>,
) -> Result<String, Box<dyn Error>> {
    let user_msg = judge_user_msg(ctx);
    let req = build_openai_chat_request(client, cfg).json(&serde_json::json!({
        "model": cfg.openai_model,
        "messages": [
            {"role": "system", "content": judge_system_prompt()},
            {"role": "user", "content": user_msg}
        ],
        "temperature": 0.3
    }));
    let response = req.send().await?.error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no analysis)")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_sources_repetition_no_sources() {
        let answer = "Some answer with no sources section.";
        let mut first = None;
        assert!(check_sources_repetition(answer, 0, &mut first).is_none());
        assert!(first.is_none());
    }

    #[test]
    fn test_sources_repetition_single_sources() {
        let answer = "Good answer.\n\n## Sources\n- [S1] https://example.com";
        let mut first = None;
        assert!(check_sources_repetition(answer, 0, &mut first).is_none());
        assert!(first.is_some()); // first occurrence recorded
    }

    #[test]
    fn test_sources_repetition_detects_second() {
        let answer = "Good answer.\n\n## Sources\n- [S1] url\n\n## Sources\n## Sources\n## Sources";
        let mut first = None;
        // First scan: records first occurrence. May find both occurrences at once
        // (returns Some) or just the first (returns None, sets `first`).
        if let Some(second_pos) = check_sources_repetition(answer, 0, &mut first) {
            // Both occurrences found in a single scan.
            let truncated = &answer[..second_pos];
            assert!(truncated.contains("- [S1] url"));
        } else {
            // First occurrence recorded in `first`; scan again to find the second.
            let first_pos = first.expect("first occurrence must be set after first scan");
            if let Some(second_pos) = check_sources_repetition(answer, first_pos + 1, &mut first) {
                let truncated = &answer[..second_pos];
                assert!(
                    truncated.contains("- [S1] url"),
                    "should preserve first sources block"
                );
                assert!(
                    !truncated[truncated.find("## Sources").unwrap() + 11..].contains("## Sources"),
                    "truncated answer should not have a second ## Sources"
                );
            } else {
                panic!("should detect second ## Sources");
            }
        }
    }

    #[test]
    fn test_sources_repetition_case_insensitive() {
        let answer = "Answer.\n## SOURCES\nlist\n## sources\nrepeat";
        let mut first = None;
        let r1 = check_sources_repetition(answer, 0, &mut first);
        if r1.is_none() {
            let r2 = check_sources_repetition(answer, first.unwrap() + 1, &mut first);
            assert!(r2.is_some(), "case-insensitive second detection failed");
        }
    }

    #[test]
    fn test_process_sse_line_emits_tagged_token() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TaggedToken>();
        let mut answer = String::new();
        let mut saw = false;
        let done = process_sse_line(
            r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#,
            &mut answer,
            false,
            &mut saw,
            Some((&tx, "with_context")),
        )
        .expect("process_sse_line should succeed");
        assert!(!done);
        assert!(saw);
        assert_eq!(answer, "hello");
        let evt = rx.try_recv().expect("expected tagged token event");
        assert_eq!(evt.stream, "with_context");
        assert_eq!(evt.delta, "hello");
    }
}
