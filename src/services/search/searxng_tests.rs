use super::*;

#[test]
fn parses_searxng_json_results() {
    let body = r#"{
        "query": "q",
        "number_of_results": 0,
        "results": [
            {"url": "https://a.example/x", "title": "A", "content": "snippet a", "engine": "startpage"},
            {"url": "https://b.example/y", "title": "B", "content": "snippet b"}
        ]
    }"#;
    let parsed: SearxResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.results.len(), 2);
    assert_eq!(parsed.results[0].url, "https://a.example/x");
    assert_eq!(parsed.results[0].title, "A");
    assert_eq!(parsed.results[0].content, "snippet a");
}

#[test]
fn missing_optional_fields_default_to_empty() {
    let body = r#"{"results": [{"url": "https://a.example/x"}]}"#;
    let parsed: SearxResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.results.len(), 1);
    assert_eq!(parsed.results[0].title, "");
    assert_eq!(parsed.results[0].content, "");
}

#[test]
fn absent_results_array_is_empty_not_error() {
    let parsed: SearxResponse = serde_json::from_str(r#"{"query":"q"}"#).unwrap();
    assert!(parsed.results.is_empty());
}

#[test]
fn time_range_maps_to_searxng_values() {
    assert_eq!(time_range_param(TimeRange::Day), Some("day"));
    assert_eq!(time_range_param(TimeRange::Week), Some("week"));
    assert_eq!(time_range_param(TimeRange::Month), Some("month"));
    assert_eq!(time_range_param(TimeRange::Year), Some("year"));
}
