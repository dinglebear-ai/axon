use super::*;

#[test]
fn crates_io_extra_fields() {
    let data = serde_json::json!({
        "crate": {
            "name": "serde",
            "max_stable_version": "1.0.219",
            "downloads": 500_000_000u64,
            "homepage": "https://serde.rs",
            "repository": "https://github.com/serde-rs/serde",
        },
        "versions": [{
            "license": "MIT OR Apache-2.0",
            "rust_version": "1.31",
            "edition": "2018",
        }],
        "keywords": [
            { "keyword": "serialization" },
            { "keyword": "serde" },
        ],
    });
    let extra = build_extra(&data, None);
    assert_eq!(extra["pkg_registry"], "crates_io");
    assert_eq!(extra["pkg_name"], "serde");
    assert_eq!(extra["pkg_language"], "rust");
    assert_eq!(extra["pkg_license"], "MIT OR Apache-2.0");
    assert_eq!(extra["crate_msrv"], "1.31");
    assert_eq!(extra["crate_edition"], "2018");
    assert!(extra["pkg_downloads"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn crates_io_extra_empty_optional_fields_absent() {
    let data = serde_json::json!({
        "crate": {
            "name": "tiny",
            "max_stable_version": "0.1.0",
            "downloads": 0u64,
        },
        "versions": [{}],
        "keywords": [],
    });
    let extra = build_extra(&data, None);
    assert!(extra.get("pkg_license").is_none());
    assert!(extra.get("pkg_keywords").is_none());
    assert!(extra.get("pkg_downloads").is_none());
    assert!(extra.get("pkg_homepage").is_none());
}

#[test]
fn matches_crate_root() {
    assert!(matches("https://crates.io/crates/serde"));
}

#[test]
fn matches_crate_with_version() {
    assert!(matches("https://crates.io/crates/serde/1.0.0"));
}

#[test]
fn matches_crate_with_patch() {
    assert!(matches("https://crates.io/crates/tokio/1.37.0"));
}

#[test]
fn no_match_crates_io_root() {
    assert!(!matches("https://crates.io/"));
}

#[test]
fn no_match_crates_io_search() {
    assert!(!matches("https://crates.io/search?q=serde"));
}

#[test]
fn no_match_wrong_host() {
    assert!(!matches("https://docs.rs/serde"));
}

#[test]
fn no_match_single_segment() {
    assert!(!matches("https://crates.io/crates"));
}

#[test]
fn fmt_num_small() {
    assert_eq!(fmt_num(0), "0");
    assert_eq!(fmt_num(999), "999");
}

#[test]
fn fmt_num_thousands() {
    assert_eq!(fmt_num(1_000), "1,000");
    assert_eq!(fmt_num(1_234_567), "1,234,567");
}

#[test]
fn strip_html_basic() {
    let html = "<h1>Hello</h1><p>World</p>";
    let text = strip_html(html);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains('<'));
}

#[test]
fn strip_html_empty() {
    assert_eq!(strip_html(""), "");
}
