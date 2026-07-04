use super::*;

#[test]
fn retry_scope_json_names_are_snake_case() {
    let cases = [
        (RetryScope::Item, "item"),
        (RetryScope::Document, "document"),
        (RetryScope::Phase, "phase"),
        (RetryScope::Job, "job"),
        (RetryScope::Provider, "provider"),
    ];
    for (scope, name) in cases {
        assert_eq!(serde_json::to_value(scope).unwrap(), name);
    }
}

#[test]
fn fail_fast_is_not_retryable() {
    let policy = RetryPolicy::fail_fast();
    assert!(!policy.retryable);
    assert_eq!(policy.retry_scope, RetryScope::Item);
}

#[test]
fn retry_policy_round_trips_serde() {
    let policy = RetryPolicy::retryable(RetryScope::Provider);
    let value = serde_json::to_value(&policy).unwrap();
    let back: RetryPolicy = serde_json::from_value(value).unwrap();
    assert_eq!(back, policy);
}
