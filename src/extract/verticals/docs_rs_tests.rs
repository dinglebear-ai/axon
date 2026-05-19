use super::*;

#[test]
fn matches_crate_only() {
    assert!(matches("https://docs.rs/serde"));
}

#[test]
fn matches_with_latest() {
    assert!(matches("https://docs.rs/tokio/latest"));
}

#[test]
fn matches_with_version() {
    assert!(matches("https://docs.rs/tokio/1.38.0"));
}

#[test]
fn matches_deep_path() {
    assert!(matches(
        "https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html"
    ));
}

#[test]
fn no_match_docs_rs_root() {
    assert!(!matches("https://docs.rs/"));
    assert!(!matches("https://docs.rs"));
}

#[test]
fn no_match_wrong_host() {
    assert!(!matches("https://crates.io/crates/serde"));
    assert!(!matches("https://docs.something.rs/serde"));
}

#[test]
fn parse_crate_only() {
    let (name, ver) = parse_crate_and_version("https://docs.rs/serde").unwrap();
    assert_eq!(name, "serde");
    assert_eq!(ver, "latest");
}

#[test]
fn parse_with_version() {
    let (name, ver) = parse_crate_and_version("https://docs.rs/tokio/1.38.0").unwrap();
    assert_eq!(name, "tokio");
    assert_eq!(ver, "1.38.0");
}

#[test]
fn parse_with_latest_explicit() {
    let (name, ver) = parse_crate_and_version("https://docs.rs/serde/latest").unwrap();
    assert_eq!(name, "serde");
    assert_eq!(ver, "latest");
}

#[test]
fn parse_deep_path_uses_second_segment_as_version() {
    // /tokio/latest/tokio/sync → version is "latest", not "tokio"
    let (name, ver) =
        parse_crate_and_version("https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html")
            .unwrap();
    assert_eq!(name, "tokio");
    assert_eq!(ver, "latest");
}

#[test]
fn parse_deep_path_with_specific_version() {
    let (name, ver) =
        parse_crate_and_version("https://docs.rs/serde/1.0.219/serde/index.html").unwrap();
    assert_eq!(name, "serde");
    assert_eq!(ver, "1.0.219");
}

#[test]
fn should_skip_kind_containers() {
    assert!(should_skip_kind("module"));
    assert!(should_skip_kind("impl"));
    assert!(should_skip_kind("use"));
    assert!(should_skip_kind("struct_field"));
    assert!(should_skip_kind("variant"));
}

#[test]
fn should_skip_kind_keeps_leaf_items() {
    assert!(!should_skip_kind("function"));
    assert!(!should_skip_kind("struct"));
    assert!(!should_skip_kind("trait"));
    assert!(!should_skip_kind("enum"));
    assert!(!should_skip_kind("macro"));
}
