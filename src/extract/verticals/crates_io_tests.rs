use super::*;

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
