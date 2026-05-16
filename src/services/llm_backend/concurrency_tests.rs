use super::*;

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
