use super::*;

#[tokio::test]
async fn fake_ask_service_ask_returns_seeded_answer() {
    let fake = FakeAskService::new();
    fake.seed_answer("what is axon?", "a rag engine");

    let request = AskRequest {
        query: Some("what is axon?".to_string()),
        diagnostics: None,
        explain: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        ask_chunk_limit: None,
        ask_full_docs: None,
        ask_max_context_chars: None,
        ask_hybrid_candidates: None,
        ask_min_relevance_score: None,
        ask_doc_chunk_limit: None,
        ask_doc_fetch_concurrency: None,
        ask_backfill_chunks: None,
        ask_candidate_limit: None,
        ask_min_citations_nontrivial: None,
        ask_authoritative_domains: None,
        ask_authoritative_boost: None,
        response_mode: None,
    };
    let result = fake.ask(request).await.expect("ask should succeed");
    assert_eq!(result.answer, "a rag engine");
}

#[tokio::test]
async fn fake_ask_service_chat_echoes_message() {
    let fake = FakeAskService::new();
    let result = fake
        .chat(ChatRequest {
            session_id: None,
            message: "hello".to_string(),
        })
        .await
        .expect("chat should succeed");
    assert!(result.reply.contains("hello"));
}

#[tokio::test]
async fn fake_ask_service_evaluate_returns_result() {
    let fake = FakeAskService::new();
    let result = fake
        .evaluate(EvaluationRequest {
            question: "what is axon?".to_string(),
        })
        .await
        .expect("evaluate should succeed");
    assert_eq!(result.query, "what is axon?");
}

#[tokio::test]
async fn fake_ask_service_suggest_returns_result() {
    let fake = FakeAskService::new();
    let request = SuggestRequest {
        focus: None,
        collection: None,
        limit: None,
        response_mode: None,
    };
    let result = fake.suggest(request).await.expect("suggest should succeed");
    assert!(result.suggestions.is_empty());
}

#[tokio::test]
async fn fake_ask_service_works_through_trait_object() {
    let fake: Arc<dyn AskService> = Arc::new(FakeAskService::new());
    let request = AskRequest {
        query: Some("hello".to_string()),
        diagnostics: None,
        explain: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        ask_chunk_limit: None,
        ask_full_docs: None,
        ask_max_context_chars: None,
        ask_hybrid_candidates: None,
        ask_min_relevance_score: None,
        ask_doc_chunk_limit: None,
        ask_doc_fetch_concurrency: None,
        ask_backfill_chunks: None,
        ask_candidate_limit: None,
        ask_min_citations_nontrivial: None,
        ask_authoritative_domains: None,
        ask_authoritative_boost: None,
        response_mode: None,
    };
    let result = fake.ask(request).await.expect("ask should succeed");
    assert_eq!(result.answer, "fake answer");
}

/// Compile-only check: `AskServiceImpl` satisfies `AskService`. Not
/// executed — constructing a real `ServiceContext` needs live services.
fn _assert_ask_service_impl<T: AskService>() {}
#[allow(dead_code)]
fn _ask_service_impl_satisfies_trait() {
    _assert_ask_service_impl::<AskServiceImpl>();
}
