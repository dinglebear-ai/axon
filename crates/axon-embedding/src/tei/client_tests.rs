use super::*;
use std::time::{Duration, Instant};

#[test]
fn is_retryable_status_covers_429_and_5xx_only() {
    assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
    assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
    assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
    assert!(is_retryable_status(StatusCode::GATEWAY_TIMEOUT));
    assert!(!is_retryable_status(StatusCode::OK));
    assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
    // 413 drives the batch-split path, not the generic retry path.
    assert!(!is_retryable_status(StatusCode::PAYLOAD_TOO_LARGE));
}

#[test]
fn is_batch_too_large_is_413_only() {
    assert!(is_batch_too_large(StatusCode::PAYLOAD_TOO_LARGE));
    assert!(!is_batch_too_large(StatusCode::UNPROCESSABLE_ENTITY));
    assert!(!is_batch_too_large(StatusCode::OK));
}

#[test]
fn retry_delay_grows_exponentially_and_caps() {
    let now = Instant::now();
    assert!(retry_delay(1, now).as_millis() >= 1000);
    assert!(retry_delay(2, now).as_millis() >= 2000);
    assert!(retry_delay(3, now).as_millis() >= 4000);
    // Capped at 60_000 + <500ms jitter.
    assert!(retry_delay(100, now).as_millis() <= 60_500);
}

#[test]
fn retry_delay_attempt_zero_does_not_panic() {
    // saturating_sub(1) clamps to 0 → base 1000ms, no u32 underflow.
    assert!(retry_delay(0, Instant::now()).as_millis() >= 1000);
}

#[test]
fn resolve_batch_size_clamps_to_valid_range() {
    // Env var is not set in this test, so config value is used and clamped.
    assert_eq!(resolve_batch_size(64), 64);
    assert_eq!(resolve_batch_size(0), 1);
    assert_eq!(resolve_batch_size(10_000), 256);
}

#[test]
fn exhausted_cooling_attaches_provider_cooling_metadata_and_marks_retryable() {
    let client = TeiClient::new(TeiClientParams {
        endpoint: "http://127.0.0.1:1".to_string(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_millis(10),
    })
    .expect("client construction performs no I/O");

    let before = Utc::now();
    let err = client.status_error(StatusCode::SERVICE_UNAVAILABLE);
    let cooled = client.with_exhausted_cooling(err);

    assert!(
        cooled.retryable,
        "an exhausted retryable error stays retryable"
    );
    let cooling = cooled
        .provider_cooling()
        .expect("retry-exhausted errors must carry ProviderCooling metadata");
    assert_eq!(cooling.provider_id.as_deref(), Some("tei"));
    assert!(cooling.cooldown_until > before);
    assert_eq!(cooled.cooldown_until, Some(cooling.cooldown_until));
}
