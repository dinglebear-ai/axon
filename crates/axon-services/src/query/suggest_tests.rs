use super::*;
use axon_llm::{CompletionRequest, CompletionRunner, CompletionTurnResult};
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

/// Test-only runner-injected variant of [`request_suggestions_from_llm`],
/// mirroring legacy `suggest::request_suggestions_from_runner`.
async fn request_suggestions_from_runner<R>(
    runner: &R,
    user_prompt: &str,
) -> Result<String, Box<dyn Error + Send + Sync>>
where
    R: CompletionRunner + ?Sized,
{
    let req = CompletionRequest::new(user_prompt)
        .system_prompt("You propose complementary documentation source targets. Output JSON only.")
        .stream(false);
    let response = axon_llm::complete_text_with_runner(runner, req).await?;
    Ok(response.text)
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

#[test]
fn url_lookup_candidates_includes_slash_and_normalized_variants() {
    let candidates = url_lookup_candidates("docs.example.com/guide");
    assert!(candidates.contains(&"docs.example.com/guide".to_string()));
    assert!(
        candidates
            .iter()
            .any(|c| c.starts_with("https://") && c.contains("docs.example.com/guide"))
    );
}

#[test]
fn suggestion_score_prefers_docs_paths_over_low_value_paths() {
    let docs_score = suggestion_score("https://example.com/docs/guide");
    let privacy_score = suggestion_score("https://example.com/privacy");
    assert!(docs_score > privacy_score);
}

#[test]
fn indexed_url_from_payload_prefers_unified_canonical_uri() {
    let payload = serde_json::json!({
        "item_canonical_uri": "https://docs.example.com/page",
        "source_canonical_uri": "https://docs.example.com",
        "url": "https://legacy.example.com/page"
    });

    assert_eq!(
        indexed_url_from_payload(&payload).as_deref(),
        Some("https://docs.example.com/page")
    );
}

#[test]
fn indexed_url_from_payload_ignores_legacy_url_field() {
    let payload = serde_json::json!({
        "url": "https://legacy.example.com/page"
    });

    assert_eq!(indexed_url_from_payload(&payload), None);
}

#[test]
fn ranked_base_urls_falls_back_to_hosts_when_domain_facet_missing() {
    let indexed = vec![
        "https://b.example.com/one".to_string(),
        "https://a.example.com/one".to_string(),
        "https://b.example.com/two".to_string(),
    ];

    assert_eq!(
        ranked_base_urls_from_context(&indexed, Vec::new()),
        vec![
            ("b.example.com".to_string(), 2),
            ("a.example.com".to_string(), 1)
        ]
    );
}
