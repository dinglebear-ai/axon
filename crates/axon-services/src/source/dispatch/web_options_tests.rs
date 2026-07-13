use axon_core::config::{Config, RenderMode};

use super::*;

#[test]
fn max_pages_override_wins_over_config() {
    let cfg = Config {
        max_pages: 10,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, Some(99));
    assert_eq!(options.get("max_pages").unwrap(), &serde_json::json!(99));
}

#[test]
fn max_pages_falls_back_to_config_when_no_override() {
    let cfg = Config {
        max_pages: 10,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None);
    assert_eq!(options.get("max_pages").unwrap(), &serde_json::json!(10));
}

#[test]
fn render_mode_round_trips_to_the_api_snake_case_form() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None);
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
    let options = web_crawl_options(&cfg, None);
    assert_eq!(
        options.get("etag_conditional").unwrap(),
        &serde_json::json!(true)
    );
}

#[test]
fn url_whitelist_and_blacklist_only_set_when_nonempty() {
    let cfg = Config::default();
    let options = web_crawl_options(&cfg, None);
    assert!(options.get("url_whitelist").is_none());
    assert!(options.get("url_blacklist").is_none());

    let cfg = Config {
        url_whitelist: vec!["^https://example\\.com".to_string()],
        exclude_path_prefix: vec!["/blocked".to_string()],
        ..Config::default()
    };
    let options = web_crawl_options(&cfg, None);
    assert_eq!(
        options.get("url_whitelist").unwrap(),
        &serde_json::json!(["^https://example\\.com"])
    );
    assert_eq!(
        options.get("url_blacklist").unwrap(),
        &serde_json::json!(["/blocked"])
    );
}
