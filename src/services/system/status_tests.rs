use super::*;

#[test]
fn status_payload_includes_expected_keys() {
    let payload = build_status_payload(&[], &[], &[], &[], &StatusTotals::default());
    assert!(payload.get("local_crawl_jobs").is_some());
    assert!(payload.get("local_ingest_jobs").is_some());
    assert!(payload.get("totals").is_some());
}
