use super::*;

#[test]
fn credentials_are_explicit_capabilities_not_ambient_state() {
    let context = VerticalContext::new(None, Vec::new());
    assert!(context.github_token().is_none());
    assert!(context.huggingface_token().is_none());
    assert!(context.reddit_credentials().is_none());
}

#[test]
fn vertical_context_is_constructed_from_public_capabilities_only() {
    let context =
        VerticalContext::new(Some("public-agent".to_string()), vec!["amazon".to_string()]);
    assert_eq!(context.ua(), "public-agent");
    assert!(context.auto_dispatch_skipped("amazon"));
    assert!(!context.auto_dispatch_skipped("docs_rs"));
}
