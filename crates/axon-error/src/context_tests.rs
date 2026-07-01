use super::*;

#[test]
fn visibility_json_names_are_snake_case() {
    assert_eq!(
        serde_json::to_value(ErrorVisibility::Public).unwrap(),
        "public"
    );
    assert_eq!(
        serde_json::to_value(ErrorVisibility::Internal).unwrap(),
        "internal"
    );
    assert_eq!(
        serde_json::to_value(ErrorVisibility::Sensitive).unwrap(),
        "sensitive"
    );
}

#[test]
fn public_entries_exclude_sensitive() {
    let ctx = ErrorContext::new()
        .public("provider", "tei")
        .insert("token", ErrorContextEntry::sensitive("REDACTED", "api_key"));

    let public: Vec<_> = ctx.public_entries().map(|(k, _)| k.clone()).collect();
    assert_eq!(public, vec!["provider".to_string()]);
    assert!(!ctx.entries["token"].is_public());
    assert_eq!(
        ctx.entries["token"].secret_class.as_deref(),
        Some("api_key")
    );
}

#[test]
fn context_round_trips_serde() {
    let ctx = ErrorContext::new()
        .public("provider", "tei")
        .insert("token", ErrorContextEntry::sensitive("REDACTED", "api_key"));
    let value = serde_json::to_value(&ctx).unwrap();
    let back: ErrorContext = serde_json::from_value(value).unwrap();
    assert_eq!(back, ctx);
}
