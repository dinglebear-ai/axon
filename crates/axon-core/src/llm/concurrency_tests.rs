use super::*;
use std::sync::Arc;

#[test]
fn completion_concurrency_defaults_to_four() {
    assert_eq!(parse_completion_concurrency_limit(None), 4);
}

#[test]
fn completion_concurrency_rejects_zero() {
    assert_eq!(parse_completion_concurrency_limit(Some("0")), 4);
}

#[test]
fn completion_concurrency_clamps_to_semaphore_max() {
    let huge = (Semaphore::MAX_PERMITS + 1).to_string();
    assert_eq!(
        parse_completion_concurrency_limit(Some(&huge)),
        Semaphore::MAX_PERMITS
    );
}

#[tokio::test]
async fn completion_limiter_is_keyed_by_backend_identity_only() {
    reset_completion_limiters_for_tests();

    let openai_key = CompletionKey::OpenAi {
        base_url: "http://one".to_string(),
        model: "gpt".to_string(),
    };
    let first = acquire_completion_permit_for_key(openai_key.clone(), 1)
        .await
        .expect("first permit");

    assert_eq!(
        available_permits_for_key(&openai_key),
        Some(0),
        "first permit should saturate the one-permit limiter"
    );

    let same_key_limit_one = completion_semaphore_for_key_for_tests(openai_key.clone(), 1);
    let same_key_limit_two = completion_semaphore_for_key_for_tests(openai_key.clone(), 2);
    assert!(
        Arc::ptr_eq(&same_key_limit_one, &same_key_limit_two),
        "changing the limit must not create a bypass bucket for the same backend",
    );

    let gemini_key = CompletionKey::Gemini {
        cmd: "gemini".to_string(),
        model: "flash".to_string(),
    };
    let second_different_backend = acquire_completion_permit_for_key(gemini_key, 1).await;
    assert!(
        second_different_backend.is_ok(),
        "different backend key should use an independent limiter"
    );

    drop(first);
}
