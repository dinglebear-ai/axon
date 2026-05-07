use super::*;
use crate::core::config::Config;
use crate::core::http::http_client;
use crate::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult,
};
use std::sync::{Arc, Mutex};

use super::super::super::streaming::{
    ask_llm_non_streaming_with_runner, ask_llm_streaming_tagged_with_runner,
    baseline_llm_non_streaming_with_runner, baseline_llm_streaming_tagged_with_runner,
};

#[allow(clippy::type_complexity)]
fn build_parallel_futures_with_runners<'a, RR, BR>(
    cfg: &'a Config,
    client: &'a reqwest::Client,
    query: &'a str,
    context: &'a str,
    rag_runner: &'a RR,
    baseline_runner: &'a BR,
) -> (
    impl Future<Output = Result<(String, u128), Box<dyn Error>>> + 'a,
    impl Future<Output = Result<(String, u128), Box<dyn Error>>> + 'a,
    mpsc::UnboundedReceiver<TaggedToken>,
)
where
    RR: AcpCompletionRunner + ?Sized + 'a,
    BR: AcpCompletionRunner + ?Sized + 'a,
{
    let (tx, rx) = mpsc::unbounded_channel::<TaggedToken>();
    let rag_tx = tx.clone();
    let baseline_tx = tx.clone();
    drop(tx);

    let rag_future = async move {
        let started = Instant::now();
        let answer = match ask_llm_streaming_tagged_with_runner(
            rag_runner,
            cfg,
            query,
            context,
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
                    ask_llm_non_streaming_with_runner(rag_runner, cfg, query, context).await?;
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
        let answer = match baseline_llm_streaming_tagged_with_runner(
            baseline_runner,
            cfg,
            query,
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
                    baseline_llm_non_streaming_with_runner(baseline_runner, cfg, query).await?;
                let _ = baseline_tx.send(TaggedToken {
                    stream: STREAM_WITHOUT_CONTEXT,
                    delta: fallback.clone(),
                });
                fallback
            }
        };
        Ok::<(String, u128), Box<dyn Error>>((answer, started.elapsed().as_millis()))
    };

    let _ = client;
    (rag_future, baseline_future, rx)
}

#[derive(Clone)]
struct MockRunner {
    observed: Arc<Mutex<Vec<AcpCompletionRequest>>>,
    stream_deltas: Vec<String>,
    stream_result: Result<String, String>,
    text_result: Result<String, String>,
}

impl MockRunner {
    fn streaming(deltas: &[&str], final_text: &str) -> Self {
        Self {
            observed: Arc::default(),
            stream_deltas: deltas.iter().map(|delta| (*delta).to_string()).collect(),
            stream_result: Ok(final_text.to_string()),
            text_result: Ok(final_text.to_string()),
        }
    }

    fn stream_failure(message: &str, fallback_text: &str) -> Self {
        Self {
            observed: Arc::default(),
            stream_deltas: Vec::new(),
            stream_result: Err(message.to_string()),
            text_result: Ok(fallback_text.to_string()),
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

#[tokio::test(flavor = "current_thread")]
async fn build_parallel_futures_with_runners_preserves_tagged_stream_tokens() {
    let cfg = Config::test_default();
    let client = http_client().expect("http client should build");
    let rag_runner = MockRunner::streaming(&["rag-", "delta"], "rag-delta");
    let baseline_runner = MockRunner::streaming(&["base-", "delta"], "base-delta");
    let (rag_future, baseline_future, mut rx) = build_parallel_futures_with_runners(
        &cfg,
        client,
        "What is ACP?",
        "Indexed context",
        &rag_runner,
        &baseline_runner,
    );

    let (rag, baseline) = tokio::join!(rag_future, baseline_future);
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push((evt.stream, evt.delta));
    }

    assert_eq!(rag.expect("rag future should succeed").0, "rag-delta");
    assert_eq!(
        baseline.expect("baseline future should succeed").0,
        "base-delta"
    );
    assert!(events.contains(&(STREAM_WITH_CONTEXT, "rag-".to_string())));
    assert!(events.contains(&(STREAM_WITH_CONTEXT, "delta".to_string())));
    assert!(events.contains(&(STREAM_WITHOUT_CONTEXT, "base-".to_string())));
    assert!(events.contains(&(STREAM_WITHOUT_CONTEXT, "delta".to_string())));
}

#[tokio::test(flavor = "current_thread")]
async fn build_parallel_futures_with_runners_tags_fallback_text_when_streaming_fails() {
    let cfg = Config::test_default();
    let client = http_client().expect("http client should build");
    let rag_runner = MockRunner::stream_failure("stream down", "rag fallback");
    let baseline_runner = MockRunner::streaming(&["base"], "base");
    let (rag_future, baseline_future, mut rx) = build_parallel_futures_with_runners(
        &cfg,
        client,
        "What is ACP?",
        "Indexed context",
        &rag_runner,
        &baseline_runner,
    );

    let (rag, baseline) = tokio::join!(rag_future, baseline_future);
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push((evt.stream, evt.delta));
    }

    assert_eq!(rag.expect("rag future should succeed").0, "rag fallback");
    assert_eq!(baseline.expect("baseline future should succeed").0, "base");
    assert!(events.contains(&(STREAM_WITH_CONTEXT, "rag fallback".to_string())));
    assert!(events.contains(&(STREAM_WITHOUT_CONTEXT, "base".to_string())));
}
