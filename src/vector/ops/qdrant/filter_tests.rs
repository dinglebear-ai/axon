use super::*;
use chrono::Utc;

// ── parse_time_filter ──────────────────────────────────────────────────────

#[test]
fn parse_days_shorthand() {
    let result = parse_time_filter("7d");
    assert!(result.is_ok(), "7d must parse: {:?}", result);
    let dt = result.unwrap();
    let diff = Utc::now() - dt;
    // Should be approximately 7 days ago (allow ±5 seconds for test execution time)
    assert!(diff.num_seconds() >= 7 * 86_400 - 5);
    assert!(diff.num_seconds() <= 7 * 86_400 + 5);
}

#[test]
fn parse_weeks_shorthand() {
    let result = parse_time_filter("2w");
    assert!(result.is_ok(), "2w must parse: {:?}", result);
    let dt = result.unwrap();
    let diff = Utc::now() - dt;
    assert!(diff.num_seconds() >= 14 * 86_400 - 5);
    assert!(diff.num_seconds() <= 14 * 86_400 + 5);
}

#[test]
fn parse_iso_date() {
    let result = parse_time_filter("2026-01-15");
    assert!(result.is_ok(), "YYYY-MM-DD must parse: {:?}", result);
    let dt = result.unwrap();
    assert_eq!(
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "2026-01-15T00:00:00Z"
    );
}

#[test]
fn parse_rfc3339() {
    let result = parse_time_filter("2026-06-01T12:00:00Z");
    assert!(result.is_ok(), "RFC3339 must parse: {:?}", result);
    let dt = result.unwrap();
    assert_eq!(
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "2026-06-01T12:00:00Z"
    );
}

#[test]
fn parse_invalid_returns_err() {
    assert!(parse_time_filter("banana").is_err());
    assert!(parse_time_filter("0d").is_err());
    assert!(parse_time_filter("-7d").is_err());
    assert!(parse_time_filter("2026-99-99").is_err());
}

// ── build_scraped_at_filter ───────────────────────────────────────────────

#[test]
fn both_none_returns_none() {
    assert!(build_scraped_at_filter(None, None).unwrap().is_none());
}

#[test]
fn since_only_builds_gte_range() {
    let f = build_scraped_at_filter(Some("2026-01-01"), None).unwrap();
    assert!(f.is_some());
    let f = f.unwrap();
    let range = &f["must"][0]["range"];
    assert!(range["gte"].as_str().is_some(), "gte must be set");
    assert!(range["lte"].is_null(), "lte must not be set for since-only");
    assert_eq!(f["must"][0]["key"].as_str(), Some("scraped_at"));
}

#[test]
fn before_only_builds_lte_range() {
    let f = build_scraped_at_filter(None, Some("2026-03-01")).unwrap();
    assert!(f.is_some());
    let f = f.unwrap();
    let range = &f["must"][0]["range"];
    assert!(range["lte"].as_str().is_some(), "lte must be set");
    assert!(
        range["gte"].is_null(),
        "gte must not be set for before-only"
    );
}

#[test]
fn both_bounds_set_correctly() {
    let f = build_scraped_at_filter(Some("2026-01-01"), Some("2026-03-01")).unwrap();
    assert!(f.is_some());
    let f = f.unwrap();
    let range = &f["must"][0]["range"];
    assert!(range["gte"].as_str().is_some());
    assert!(range["lte"].as_str().is_some());
}

#[test]
fn invalid_since_returns_error() {
    let err = build_scraped_at_filter(Some("not-a-date"), None).unwrap_err();
    assert!(
        err.contains("--since parse error"),
        "invalid since must return an error, got: {err}"
    );
}

#[test]
fn invalid_before_returns_error_even_with_valid_since() {
    let err = build_scraped_at_filter(Some("2026-01-01"), Some("not-a-date")).unwrap_err();
    assert!(
        err.contains("--before parse error"),
        "invalid before must return an error, got: {err}"
    );
}

#[test]
fn shorthand_since_produces_valid_rfc3339_in_filter() {
    let f = build_scraped_at_filter(Some("7d"), None).unwrap().unwrap();
    let gte = f["must"][0]["range"]["gte"].as_str().unwrap();
    // Must be parseable as RFC3339
    let parsed = DateTime::parse_from_rfc3339(gte);
    assert!(parsed.is_ok(), "gte must be valid RFC3339: {gte}");
}

#[test]
fn build_schema_version_filter_none_returns_none() {
    assert!(build_schema_version_filter(None).is_none());
}

#[test]
fn build_schema_version_filter_some_emits_range_gte() {
    let f = build_schema_version_filter(Some(2)).expect("filter");
    let must = f["must"].as_array().expect("must array");
    assert_eq!(must.len(), 1);
    assert_eq!(must[0]["key"].as_str(), Some("payload_schema_version"));
    assert_eq!(must[0]["range"]["gte"].as_u64(), Some(2));
}

#[test]
fn schema_version_filter_composes_with_scraped_at() {
    let scraped = build_scraped_at_filter(Some("2026-01-01"), None)
        .unwrap()
        .unwrap();
    let version = build_schema_version_filter(Some(2)).unwrap();
    let combined = combine_must_filters(&[scraped, version]);
    let must = combined["must"].as_array().unwrap();
    assert_eq!(must.len(), 2);
    // Both keys must be present after composition.
    let keys: Vec<&str> = must.iter().filter_map(|m| m["key"].as_str()).collect();
    assert!(keys.contains(&"scraped_at"));
    assert!(keys.contains(&"payload_schema_version"));
}

#[test]
fn combine_must_filters_concatenates_conditions() {
    let combined = combine_must_filters(&[
        url_filter("https://example.com/a"),
        build_scraped_at_filter(Some("2026-01-01"), None)
            .unwrap()
            .unwrap(),
    ]);
    let must = combined["must"].as_array().unwrap();
    assert_eq!(must.len(), 2);
    assert_eq!(must[0]["key"].as_str(), Some("url"));
    assert_eq!(must[1]["key"].as_str(), Some("scraped_at"));
}
