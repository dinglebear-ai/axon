/// Regression tests for Task 3.1: System/stats/doctor/status CLI handlers
/// routed through the services layer.
///
/// These tests verify:
/// 1. The service type mapping functions produce correct output shapes that
///    the CLI handlers can consume after the rewire.
/// 2. The `StatusResult` type shape matches what `run_status` needs.
/// 3. The `DoctorResult` type shape matches what `run_doctor` needs.
///
/// These tests run without live services — they exercise the pure mapping
/// helpers and type constructors that the handlers now depend on.
use axon::services::system::{
    map_doctor_payload, map_domains_payload, map_sources_payload, map_stats_payload,
};
use axon::services::types::{
    DoctorResult, DomainFacet, DomainsResult, Pagination, SourcesResult, StatsResult, StatusResult,
    StatusTotals,
};

// ── SourcesResult shape ───────────────────────────────────────────────────────

#[test]
fn sources_result_fields_match_cli_handler_expectations() {
    let payload = serde_json::json!({
        "count": 5,
        "limit": 5,
        "offset": 0,
        "urls": [
            {"url": "https://docs.example.com/guide", "chunks": 8},
            {"url": "https://docs.example.com/api", "chunks": 3}
        ]
    });
    let result: SourcesResult = map_sources_payload(&payload).expect("valid payload");
    assert_eq!(result.count, 5);
    assert_eq!(result.limit, 5);
    assert_eq!(result.offset, 0);
    assert_eq!(result.urls.len(), 2);
    assert_eq!(result.urls[0].0, "https://docs.example.com/guide");
    assert_eq!(result.urls[0].1, 8);
}

#[test]
fn sources_result_empty_urls_is_valid() {
    let payload = serde_json::json!({
        "count": 0,
        "limit": 10,
        "offset": 0,
        "urls": []
    });
    let result: SourcesResult = map_sources_payload(&payload).expect("valid empty payload");
    assert_eq!(result.count, 0);
    assert!(result.urls.is_empty());
}

// ── DomainsResult shape ───────────────────────────────────────────────────────

#[test]
fn domains_result_fields_match_cli_handler_expectations() {
    let payload = serde_json::json!({
        "domains": [
            {"domain": "example.com", "vectors": 42},
            {"domain": "docs.example.com", "vectors": 7}
        ],
        "limit": 50,
        "offset": 0
    });
    let result: DomainsResult = map_domains_payload(&payload).expect("valid domains payload");
    assert_eq!(result.limit, 50);
    assert_eq!(result.offset, 0);
    assert_eq!(result.domains.len(), 2);

    let first: &DomainFacet = &result.domains[0];
    assert_eq!(first.domain, "example.com");
    assert_eq!(first.vectors, 42);
}

#[test]
fn domains_result_empty_domains_is_valid() {
    let payload = serde_json::json!({
        "domains": [],
        "limit": 10,
        "offset": 0
    });
    let result: DomainsResult = map_domains_payload(&payload).expect("valid empty domains");
    assert!(result.domains.is_empty());
}

// ── StatsResult shape ─────────────────────────────────────────────────────────

#[test]
fn stats_result_payload_field_is_forwarded_unchanged() {
    let raw = serde_json::json!({
        "collection": "cortex",
        "points_count": 2_570_000u64,
        "indexed_vectors_count": 2_570_000u64,
        "status": "green"
    });
    let result: StatsResult = map_stats_payload(raw.clone());
    assert_eq!(result.payload["collection"], "cortex");
    assert_eq!(result.payload["points_count"], 2_570_000u64);
}

#[test]
fn stats_result_payload_preserves_counts_key() {
    let raw = serde_json::json!({
        "counts": {
            "crawls": 10,
            "embeds": 20,
            "queries": 5
        }
    });
    let result: StatsResult = map_stats_payload(raw);
    assert_eq!(result.payload["counts"]["crawls"], 10);
    assert_eq!(result.payload["counts"]["embeds"], 20);
}

// ── DoctorResult shape ────────────────────────────────────────────────────────

#[test]
fn doctor_result_all_ok_field_reachable_from_payload() {
    let raw = serde_json::json!({
        "all_ok": true,
        "services": {
            "qdrant": {"ok": true},
            "tei": {"ok": true}
        }
    });
    let result: DoctorResult = map_doctor_payload(raw);
    assert_eq!(result.payload["all_ok"], true);
    assert_eq!(result.payload["services"]["qdrant"]["ok"], true);
}

#[test]
fn doctor_result_not_ok_when_service_down() {
    let raw = serde_json::json!({
        "all_ok": false,
        "services": {
            "qdrant": {"ok": false, "detail": "connection refused"}
        }
    });
    let result: DoctorResult = map_doctor_payload(raw);
    assert!(!result.payload["all_ok"].as_bool().unwrap_or(true));
}

// ── StatusResult shape ────────────────────────────────────────────────────────

#[test]
fn status_result_payload_and_text_are_both_present() {
    let payload = serde_json::json!({
        "local_crawl_jobs": [],
        "local_extract_jobs": [],
        "local_embed_jobs": [],
        "local_ingest_jobs": []
    });
    let text = "Axon Status\ncrawl jobs:   0\nextract jobs: 0".to_string();
    let result = StatusResult {
        payload: payload.clone(),
        text: text.clone(),
        totals: StatusTotals::default(),
        degraded: false,
        errors: Vec::new(),
    };
    assert!(result.payload.get("local_crawl_jobs").is_some());
    assert!(result.text.contains("Axon Status"));
}

// ── Pagination type ───────────────────────────────────────────────────────────

#[test]
fn pagination_constructed_for_sources_call() {
    let p = Pagination {
        limit: 100_000,
        offset: 0,
    };
    assert_eq!(p.limit, 100_000);
    assert_eq!(p.offset, 0);
}

#[test]
fn pagination_with_offset_constructed_correctly() {
    let p = Pagination {
        limit: 50,
        offset: 25,
    };
    assert_eq!(p.limit, 50);
    assert_eq!(p.offset, 25);
}
