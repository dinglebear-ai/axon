use super::super::url_utils::MapScope;
use super::*;
use std::collections::HashMap;

fn test_collector_config(scope: Option<MapScope>) -> CollectorConfig {
    CollectorConfig {
        markdown_dir: std::env::temp_dir(),
        manifest_path: std::env::temp_dir().join("collector-manifest.jsonl"),
        min_chars: 10,
        drop_thin: false,
        exclude_path_prefix: Vec::new(),
        scope,
        progress_tx: None,
        previous_manifest: Arc::new(HashMap::new()),
        selector_config: None,
        chrome_ws_url: None,
        chrome_timeout_secs: 1,
        output_dir: std::env::temp_dir(),
        ladder_thresholds: crate::core::content::LadderThresholds {
            strategy1: 30,
            strategy2: 200,
            body_multiplier: 2.0,
        },
        antibot_max_scan_bytes: 150_000,
        structured_max_bytes: 65_536,
    }
}

#[test]
fn canonicalize_and_track_page_rejects_same_host_root_outside_project_scope() {
    let col = test_collector_config(Some(MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    }));
    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen = HashSet::new();

    let url = canonicalize_and_track_page(
        "https://example.github.io/",
        &col,
        &mut summary,
        &mut urls,
        &mut seen,
    );

    assert!(url.is_none());
    assert_eq!(summary.pages_seen, 0);
    assert!(urls.is_empty());
}

#[test]
fn canonicalize_and_track_page_accepts_in_scope_project_page() {
    let col = test_collector_config(Some(MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    }));
    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen = HashSet::new();

    let url = canonicalize_and_track_page(
        "https://example.github.io/project/docs/",
        &col,
        &mut summary,
        &mut urls,
        &mut seen,
    );

    assert_eq!(
        url.as_deref(),
        Some("https://example.github.io/project/docs")
    );
    assert_eq!(summary.pages_seen, 1);
    assert!(urls.contains("https://example.github.io/project/docs"));
}
