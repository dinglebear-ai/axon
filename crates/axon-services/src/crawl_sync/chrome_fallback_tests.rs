use super::*;

#[test]
fn memory_abort_propagates_other_chrome_errors_swallowed() {
    // A memory-guard abort must propagate so the crawl fails loudly instead of
    // silently degrading to the HTTP result.
    assert!(chrome_fallback_error_propagates(
        "crawl memory guard tripped for https://example.com: rss=900 bytes total=1000 bytes"
    ));
    // Ordinary Chrome failures are recoverable and must NOT propagate.
    assert!(!chrome_fallback_error_propagates("net::ERR_TIMED_OUT"));
    assert!(!chrome_fallback_error_propagates(
        "chrome fallback failed for unrelated reasons"
    ));
}
