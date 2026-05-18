use super::*;
use crate::core::config::CommandKind;
use crate::services::types::ServiceTimeRange;

fn make_search_cfg(key: &str, query: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Search;
    cfg.positional = vec![query.to_string()];
    cfg.tavily_api_key = key.to_string();
    cfg
}

#[test]
fn search_cfg_time_range_defaults_to_none() {
    let cfg = make_search_cfg("tvly-key", "rust async");
    assert!(
        cfg.search_time_range.is_none(),
        "search_time_range should default to None"
    );
}

#[test]
fn parse_search_time_range_supports_known_values() {
    assert!(matches!(
        parse_service_time_range(Some("day")),
        Some(ServiceTimeRange::Day)
    ));
    assert!(matches!(
        parse_service_time_range(Some("week")),
        Some(ServiceTimeRange::Week)
    ));
    assert!(matches!(
        parse_service_time_range(Some("month")),
        Some(ServiceTimeRange::Month)
    ));
    assert!(matches!(
        parse_service_time_range(Some("year")),
        Some(ServiceTimeRange::Year)
    ));
}

#[test]
fn parse_search_time_range_rejects_unknown_values() {
    assert!(parse_service_time_range(Some("decade")).is_none());
    assert!(parse_service_time_range(Some("")).is_none());
    assert!(parse_service_time_range(None).is_none());
}

#[tokio::test]
async fn run_search_rejects_empty_tavily_key() {
    // run_search bails before touching service_context when the key is empty,
    // so we can use the search_crawl test helpers directly.
    use crate::services::search_crawl::tests::make_noop_ctx;
    let cfg = make_search_cfg("", "rust async");
    let ctx = make_noop_ctx();
    let err = run_search(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string().contains("TAVILY_API_KEY"),
        "expected TAVILY_API_KEY error, got: {err}"
    );
}

#[test]
fn summarize_snippet_collapses_whitespace_and_truncates() {
    let snippet = format!("first\n\nsecond\t{}", "word ".repeat(80));
    let got = summarize_snippet(&snippet);

    assert!(!got.contains('\n'));
    assert!(!got.contains('\t'));
    assert!(got.len() <= HUMAN_SNIPPET_LIMIT + 3);
    assert!(got.ends_with("..."));
}

#[test]
fn summarize_snippet_keeps_short_text() {
    assert_eq!(summarize_snippet("short\nsnippet"), "short snippet");
}
