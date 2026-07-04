use super::{
    already_indexed, filter_new_suggestions, parse_suggestions_from_llm,
    request_suggestions_from_runner,
};
use axon_llm::{CompletionRequest, CompletionRunner, CompletionTurnResult};
use std::collections::HashSet;
use std::error::Error;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeCompletionRunner {
    captured_requests: Arc<Mutex<Vec<CompletionRequest>>>,
    result: CompletionTurnResult,
}

#[async_trait::async_trait]
impl CompletionRunner for FakeCompletionRunner {
    async fn complete_text(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionTurnResult, Box<dyn Error + Send + Sync>> {
        self.captured_requests
            .lock()
            .expect("lock request capture")
            .push(req);
        Ok(self.result.clone())
    }

    async fn complete_streaming<F>(
        &self,
        _req: CompletionRequest,
        _on_delta: &mut F,
    ) -> Result<CompletionTurnResult, Box<dyn Error + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn Error + Send + Sync>> + Send,
    {
        unreachable!("suggestions request path should use complete_text")
    }
}

#[test]
fn parses_json_suggestions() {
    let input = r#"{
      "suggestions": [
        {"url":"https://docs.example.com/getting-started","reason":"Core onboarding guide"},
        {"url":"https://api.example.com/reference","reason":"API endpoint docs"}
      ]
    }"#;
    let parsed = parse_suggestions_from_llm(input);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].url, "https://docs.example.com/getting-started");
    assert_eq!(parsed[1].url, "https://api.example.com/reference");
}

#[test]
fn parses_url_tokens_when_json_is_missing() {
    let input = "Try https://docs.rs/spider and https://doc.rust-lang.org/book/.";
    let parsed = parse_suggestions_from_llm(input);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].url, "https://docs.rs/spider");
    assert_eq!(parsed[1].url, "https://doc.rust-lang.org/book/");
}

#[test]
fn rejects_already_indexed_url_variants() {
    let mut indexed = HashSet::new();
    indexed.insert("https://docs.example.com/guide".to_string());
    assert!(already_indexed("https://docs.example.com/guide/", &indexed));
    assert!(!already_indexed(
        "https://docs.example.com/changelog",
        &indexed
    ));
}

#[test]
fn filter_prefers_high_value_urls_and_diversifies_hosts() {
    let mut indexed = HashSet::new();
    indexed.insert("https://docs.a.com/old".to_string());
    let content = r#"{
      "suggestions": [
        {"url":"https://a.com/privacy","reason":"low value"},
        {"url":"https://docs.a.com/reference/api","reason":"high value"},
        {"url":"https://docs.b.com/guide","reason":"high value"},
        {"url":"https://a.com/news","reason":"low value"}
      ]
    }"#;
    let (accepted, _rejected) = filter_new_suggestions(content, &indexed, 2);
    assert_eq!(accepted.len(), 2);
    assert_eq!(accepted[0].url, "https://docs.a.com/reference/api");
    assert_eq!(accepted[1].url, "https://docs.b.com/guide");
}

#[tokio::test]
async fn request_suggestions_from_runner_reads_gateway_text() {
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeCompletionRunner {
        captured_requests: Arc::clone(&captured_requests),
        result: CompletionTurnResult {
            text: r#"{"suggestions":[{"url":"https://docs.example.com/guide","reason":"Gemini headless text"}]}"#.to_string(),
            usage: None,
        },
    };

    let response = request_suggestions_from_runner(&runner, "docs focus")
        .await
        .expect("runner response should be read");

    assert_eq!(
        response,
        r#"{"suggestions":[{"url":"https://docs.example.com/guide","reason":"Gemini headless text"}]}"#
    );

    let captured = captured_requests.lock().expect("request capture lock");
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].user_prompt, "docs focus");
}
