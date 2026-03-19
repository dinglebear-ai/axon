use super::*;
use crate::crates::core::config::Config;
use crate::crates::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Clone)]
struct MockRunner {
    observed: Arc<Mutex<Vec<AcpCompletionRequest>>>,
    stream_deltas: Vec<String>,
    stream_result: Result<String, String>,
    text_result: Result<String, String>,
}

impl MockRunner {
    fn with_streaming(deltas: &[&str], final_text: &str) -> Self {
        Self {
            observed: Arc::default(),
            stream_deltas: deltas.iter().map(|delta| (*delta).to_string()).collect(),
            stream_result: Ok(final_text.to_string()),
            text_result: Ok(final_text.to_string()),
        }
    }

    fn with_text(final_text: &str) -> Self {
        Self {
            observed: Arc::default(),
            stream_deltas: Vec::new(),
            stream_result: Ok(final_text.to_string()),
            text_result: Ok(final_text.to_string()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for MockRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>> {
        self.observed.lock().expect("lock poisoned").push(req);
        match &self.text_result {
            Ok(text) => Ok(AcpCompletionTurnResult {
                text: text.clone(),
                usage: None,
            }),
            Err(err) => Err(std::io::Error::other(err.clone()).into()),
        }
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn Error>> + Send,
    {
        self.observed.lock().expect("lock poisoned").push(req);
        for delta in &self.stream_deltas {
            on_delta(delta)?;
        }
        match &self.stream_result {
            Ok(text) => Ok(AcpCompletionTurnResult {
                text: text.clone(),
                usage: None,
            }),
            Err(err) => Err(std::io::Error::other(err.clone()).into()),
        }
    }
}

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
    assert!(first.is_some());
}

#[test]
fn test_sources_repetition_detects_second() {
    let answer = "Good answer.\n\n## Sources\n- [S1] url\n\n## Sources\n## Sources\n## Sources";
    let mut first = None;
    if let Some(second_pos) = check_sources_repetition(answer, 0, &mut first) {
        let truncated = &answer[..second_pos];
        assert!(truncated.contains("- [S1] url"));
    } else {
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

#[tokio::test(flavor = "current_thread")]
async fn ask_llm_streaming_with_runner_builds_acp_request_and_collects_tokens() {
    let cfg = Config::test_default();
    let runner = MockRunner::with_streaming(&["hello ", "there"], "hello there");

    let answer = ask_llm_streaming_with_runner(&runner, &cfg, "How?", "Context block", false)
        .await
        .expect("streaming ask should succeed");

    assert_eq!(answer, "hello there");
    let observed = runner.observed.lock().expect("lock poisoned");
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0],
        AcpCompletionRequest::new("Question: How?\n\nContext:\nContext block")
            .system_prompt(ASK_RAG_SYSTEM_PROMPT)
            .model("test-model")
            .stream(true)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn baseline_llm_non_streaming_with_runner_builds_acp_request() {
    let cfg = Config::test_default();
    let runner = MockRunner::with_text("baseline answer");

    let answer = baseline_llm_non_streaming_with_runner(&runner, &cfg, "What changed?")
        .await
        .expect("baseline completion should succeed");

    assert_eq!(answer, "baseline answer");
    let observed = runner.observed.lock().expect("lock poisoned");
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0],
        AcpCompletionRequest::new("What changed?")
            .system_prompt(BASELINE_SYSTEM_PROMPT)
            .model("test-model")
            .stream(false)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn judge_llm_non_streaming_with_runner_builds_acp_request() {
    let cfg = Config::test_default();
    let runner = MockRunner::with_text("judge answer");
    let judge_ctx = JudgeContext {
        query: "Which path is correct?",
        rag_answer: "RAG answer [S1]",
        baseline_answer: "Baseline answer",
        reference_chunks: "[R1] docs",
        rag_sources_list: "- [S1] https://example.com",
        ref_quality_note: "",
        rag_elapsed_ms: 12,
        baseline_elapsed_ms: 7,
        source_count: 1,
        context_chars: 42,
    };

    let answer = judge_llm_non_streaming_with_runner(&runner, &cfg, &judge_ctx)
        .await
        .expect("judge completion should succeed");

    assert_eq!(answer, "judge answer");
    let observed = runner.observed.lock().expect("lock poisoned");
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0],
        AcpCompletionRequest::new(judge_user_msg(&judge_ctx))
            .system_prompt(judge_system_prompt())
            .model("test-model")
            .stream(false)
    );
}
