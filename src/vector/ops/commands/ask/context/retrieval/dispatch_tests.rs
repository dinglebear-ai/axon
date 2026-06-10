use super::*;

#[test]
fn batch_fallback_warning_is_user_facing() {
    let warning = batch_fallback_warning();

    assert!(warning.contains("qdrant batch dual-search failed"));
    assert!(warning.contains("falling back to parallel-single"));
    assert!(!warning.contains("temporary qdrant outage"));
}
