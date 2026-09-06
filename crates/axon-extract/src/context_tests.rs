use super::*;
use reqwest::Client;

#[test]
fn credentials_are_explicit_capabilities_not_ambient_state() {
    let context = VerticalContext::new(None, Vec::new(), Client::new());
    assert!(context.github_token().is_none());
    assert!(context.huggingface_token().is_none());
    assert!(context.reddit_credentials().is_none());
}

#[test]
fn vertical_context_is_constructed_from_public_capabilities_only() {
    let context = VerticalContext::new(
        Some("public-agent".to_string()),
        vec!["amazon".to_string()],
        Client::new(),
    );
    assert_eq!(context.ua(), "public-agent");
    assert!(context.auto_dispatch_skipped("amazon"));
    assert!(!context.auto_dispatch_skipped("docs_rs"));
}

#[test]
fn vertical_context_uses_the_injected_http_provider() {
    let client = Client::builder()
        .user_agent("adapter-owned-client")
        .build()
        .unwrap();
    let context = VerticalContext::new(None, Vec::new(), client);
    let request = context
        .http_client()
        .get("https://example.com")
        .build()
        .unwrap();
    assert_eq!(
        request.headers().get(reqwest::header::USER_AGENT).unwrap(),
        "adapter-owned-client"
    );
}
