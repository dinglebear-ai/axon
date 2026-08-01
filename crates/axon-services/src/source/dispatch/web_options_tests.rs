use axon_core::config::{Config, RenderMode};
use std::path::PathBuf;

use super::*;

#[test]
fn max_pages_override_wins_over_config() {
    let cfg = Config {
        max_pages: 10,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, Some(99), None);
    assert_eq!(options.get("max_pages").unwrap(), &serde_json::json!(99));
}

#[test]
fn max_pages_falls_back_to_config_when_no_override() {
    let cfg = Config {
        max_pages: 10,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(options.get("max_pages").unwrap(), &serde_json::json!(10));
}

#[test]
fn max_depth_override_wins_over_config() {
    let cfg = Config {
        max_depth: 4,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, Some(2));
    assert_eq!(options.get("max_depth").unwrap(), &serde_json::json!(2));
}

#[test]
fn render_mode_round_trips_to_the_api_snake_case_form() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(
        options.get("render_mode").unwrap(),
        &serde_json::json!("auto_switch")
    );
}

#[test]
fn etag_conditional_threads_into_validated_options() {
    let cfg = Config {
        etag_conditional: true,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(
        options.get("etag_conditional").unwrap(),
        &serde_json::json!(true)
    );
}

#[test]
fn llms_txt_discovery_options_thread_into_validated_options() {
    let cfg = Config {
        discover_llms_txt: false,
        max_llms_txt_urls: 17,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(
        options.get("discover_llms_txt").unwrap(),
        &serde_json::json!(false)
    );
    assert_eq!(
        options.get("max_llms_txt_urls").unwrap(),
        &serde_json::json!(17)
    );
}

#[test]
fn url_whitelist_and_blacklist_only_set_when_nonempty() {
    let cfg = Config::default();
    let options = web_crawl_options(&cfg, None, None);
    assert!(options.get("url_whitelist").is_none());
    assert!(options.get("url_blacklist").is_none());

    let cfg = Config {
        url_whitelist: vec!["^https://example\\.com".to_string()],
        exclude_path_prefix: vec!["/blocked".to_string()],
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(
        options.get("url_whitelist").unwrap(),
        &serde_json::json!(["^https://example\\.com"])
    );
    assert_eq!(
        options.get("url_blacklist").unwrap(),
        &serde_json::json!(["/blocked"])
    );
}

#[test]
fn warc_automation_and_headers_thread_into_validated_options() {
    let cfg = Config {
        warc_output: Some(PathBuf::from("/tmp/source.warc")),
        automation_script: Some(PathBuf::from("/tmp/automation.json")),
        custom_headers: vec!["X-Test: ok".to_string()],
        ..Config::default()
    };

    let options = web_crawl_options(&cfg, None, None);

    assert_eq!(
        options.get("warc_path").unwrap(),
        &serde_json::json!("/tmp/source.warc")
    );
    assert_eq!(
        options.get("automation_script").unwrap(),
        &serde_json::json!("/tmp/automation.json")
    );
    assert_eq!(
        options.get("headers").unwrap(),
        &serde_json::json!({"X-Test": "ok"})
    );
}

#[test]
fn robots_cache_and_vertical_ttls_thread_into_validated_options() {
    let cfg = Config {
        respect_robots: true,
        etag_conditional: true,
        vertical_cache_ttl_secs: [("github".to_string(), 60)].into_iter().collect(),
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None, None);
    assert_eq!(
        options.get("respect_robots"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        options.get("cache_policy"),
        Some(&serde_json::json!("revalidate"))
    );
    assert_eq!(
        options.get("vertical_cache_ttl_secs"),
        Some(&serde_json::json!({"github": 60}))
    );
}

#[test]
fn advertised_web_globals_thread_into_validated_options() {
    let cfg = Config {
        cache: true,
        cache_http_only: true,
        normalize: true,
        block_assets: true,
        chrome_wait_for_selector: Some("#ready".to_string()),
        root_selector: Some("main".to_string()),
        exclude_selector: Some("aside".to_string()),
        chrome_screenshot: true,
        path_budgets: vec![("/docs".to_string(), 7)],
        format: axon_core::config::ScrapeFormat::RawHtml,
        sitemap_only: true,
        output_dir: PathBuf::from("/tmp/axon-output"),
        ..Config::default()
    };

    let options = web_crawl_options(&cfg, None, None);

    assert_eq!(options.get("cache_policy"), Some(&serde_json::json!("use")));
    assert_eq!(
        options.get("cache_http_only"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(options.get("normalize"), Some(&serde_json::json!(true)));
    assert_eq!(options.get("block_assets"), Some(&serde_json::json!(true)));
    assert_eq!(
        options.get("chrome_wait_for_selector"),
        Some(&serde_json::json!("#ready"))
    );
    assert_eq!(
        options.get("root_selector"),
        Some(&serde_json::json!("main"))
    );
    assert_eq!(
        options.get("exclude_selector"),
        Some(&serde_json::json!("aside"))
    );
    assert_eq!(
        options.get("chrome_screenshot"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        options.get("path_budgets"),
        Some(&serde_json::json!({"/docs": 7}))
    );
    assert_eq!(options.get("format"), Some(&serde_json::json!("rawHtml")));
    assert_eq!(options.get("sitemap_only"), Some(&serde_json::json!(true)));
    assert_eq!(
        options.get("output_dir"),
        Some(&serde_json::json!("/tmp/axon-output"))
    );
}

#[test]
fn caller_web_options_reject_automation_without_local_execute() {
    let mut base = MetadataMap::new();
    let mut caller = MetadataMap::new();
    caller.insert(
        "automation_script".to_string(),
        serde_json::json!("/tmp/automation.json"),
    );
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

    let err = merge_caller_web_options(&mut base, &caller, Some(&snapshot))
        .expect_err("automation_script requires elevated scopes");

    assert_eq!(err.code.0, "auth.scope_required");
    assert!(base.get("automation_script").is_none());
}

#[test]
fn caller_web_options_allow_automation_for_local_execute() {
    let mut base = MetadataMap::new();
    let mut caller = MetadataMap::new();
    caller.insert(
        "automation_script".to_string(),
        serde_json::json!("/tmp/automation.json"),
    );
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![
        AuthScope::Read,
        AuthScope::Write,
        AuthScope::Local,
        AuthScope::Execute,
    ];

    merge_caller_web_options(&mut base, &caller, Some(&snapshot)).expect("scopes allow option");

    assert_eq!(
        base.get("automation_script").unwrap(),
        &serde_json::json!("/tmp/automation.json")
    );
}
