use super::thin_client::{page_path_i64, should_use_mcp_thin_client};
use crate::core::config::Config;

#[test]
fn mcp_uses_thin_client_when_server_url_is_set() {
    let mut cfg = Config::default_minimal();
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg.local_mode = false;

    assert!(should_use_mcp_thin_client(&cfg));
}

#[test]
fn mcp_stays_local_when_local_mode_is_forced() {
    let mut cfg = Config::default_minimal();
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg.local_mode = true;

    assert!(!should_use_mcp_thin_client(&cfg));
}

#[test]
fn mcp_lifecycle_list_preserves_pagination_query() {
    assert_eq!(
        page_path_i64("/v1/crawl", Some(25), Some(50)),
        "/v1/crawl?limit=25&offset=50"
    );
}
