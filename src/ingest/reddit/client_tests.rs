use super::*;
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use std::time::Duration;

#[test]
fn retry_after_seconds_wins_when_within_cap() {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("17"));

    assert_eq!(retry_delay_for_429(&headers, 1), Duration::from_secs(17));
}

#[test]
fn retry_after_seconds_is_capped() {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("999"));

    assert_eq!(retry_delay_for_429(&headers, 1), Duration::from_secs(60));
}

#[test]
fn invalid_retry_after_uses_exponential_fallback() {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("later"));

    assert_eq!(retry_delay_for_429(&headers, 2), Duration::from_secs(4));
}
