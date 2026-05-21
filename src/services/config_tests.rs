use super::*;
use std::io::ErrorKind;
use tempfile::TempDir;

#[test]
fn env_round_trip_set_get_unset() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    set_env_entry(&path, "QDRANT_URL", "http://localhost:53333").unwrap();
    set_env_entry(&path, "TAVILY_API_KEY", "secret-value").unwrap();
    let entries = read_env_entries(&path).unwrap();
    assert_eq!(
        entries.get("QDRANT_URL").map(String::as_str),
        Some("http://localhost:53333")
    );
    assert_eq!(
        entries.get("TAVILY_API_KEY").map(String::as_str),
        Some("secret-value")
    );

    let removed = unset_env_entry(&path, "TAVILY_API_KEY").unwrap();
    assert!(removed);
    assert!(
        !read_env_entries(&path)
            .unwrap()
            .contains_key("TAVILY_API_KEY")
    );
    assert!(!unset_env_entry(&path, "TAVILY_API_KEY").unwrap());
}

#[test]
fn env_rejects_invalid_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    let err = set_env_entry(&path, "1bad-key", "x").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
}

#[test]
fn env_quotes_values_with_spaces() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    set_env_entry(&path, "QUOTED", "hello world").unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("QUOTED='hello world'"));
}

#[test]
fn raw_env_text_validates_before_write() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");

    write_env_text(
        &path,
        "QDRANT_URL=http://localhost:53333\nTAVILY_API_KEY='secret value'\n",
    )
    .unwrap();
    assert_eq!(
        read_env_entries(&path)
            .unwrap()
            .get("TAVILY_API_KEY")
            .map(String::as_str),
        Some("secret value")
    );

    let err = write_env_text(&path, "BROKEN='unterminated\n").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn toml_set_get_unset_nested() {
    let mut doc = toml_edit::DocumentMut::new();
    set_toml_entry(&mut doc, "ask.cache.enabled", "true").unwrap();
    set_toml_entry(&mut doc, "ask.cache.ttl-secs", "120").unwrap();
    set_toml_entry(&mut doc, "search.collection", "cortex").unwrap();

    assert_eq!(
        get_toml_entry(&doc, "ask.cache.enabled").as_deref(),
        Some("true")
    );
    assert_eq!(
        get_toml_entry(&doc, "ask.cache.ttl-secs").as_deref(),
        Some("120")
    );
    assert_eq!(
        get_toml_entry(&doc, "search.collection").as_deref(),
        Some("cortex")
    );

    let flat = flatten_toml(&doc);
    assert_eq!(
        flat.get("ask.cache.enabled").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        flat.get("search.collection").map(String::as_str),
        Some("cortex")
    );

    assert!(unset_toml_entry(&mut doc, "ask.cache.ttl-secs").unwrap());
    assert!(get_toml_entry(&doc, "ask.cache.ttl-secs").is_none());
    assert!(!unset_toml_entry(&mut doc, "ask.cache.ttl-secs").unwrap());
}

#[test]
fn toml_scalar_parsing_picks_correct_types() {
    let mut doc = toml_edit::DocumentMut::new();
    set_toml_entry(&mut doc, "x.bool", "true").unwrap();
    set_toml_entry(&mut doc, "x.int", "42").unwrap();
    set_toml_entry(&mut doc, "x.float", "3.14").unwrap();
    set_toml_entry(&mut doc, "x.str", "hello").unwrap();
    let raw = doc.to_string();
    assert!(raw.contains("bool = true"));
    assert!(raw.contains("int = 42"));
    assert!(raw.contains("float = 3.14"));
    assert!(raw.contains("str = \"hello\""));
}

#[test]
fn secret_detection_matches_registry_and_heuristic() {
    assert!(is_secret_env_key("TAVILY_API_KEY"));
    assert!(is_secret_env_key("GITHUB_TOKEN"));
    assert!(is_secret_env_key("REDDIT_CLIENT_SECRET"));
    assert!(is_secret_env_key("CUSTOM_PASSWORD"));
    assert!(is_secret_env_key("ANYTHING_TOKEN"));
    assert!(!is_secret_env_key("QDRANT_URL"));
    assert!(!is_secret_env_key("TEI_URL"));
}

#[test]
fn redact_returns_empty_for_empty_value() {
    assert_eq!(redact(""), "");
    assert_eq!(redact("hello"), "***");
}
