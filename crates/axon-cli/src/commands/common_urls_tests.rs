use super::{MAX_EXPANSION_TOTAL, expand_url_glob_seed, start_url_from_cfg, truncate_chars};
use axon_core::config::{CommandKind, Config};

#[test]
fn truncate_chars_multibyte() {
    assert_eq!(truncate_chars("hello", 5), "hello");
    assert_eq!(truncate_chars("hello", 3), "hel");
    assert_eq!(truncate_chars("héllo", 3), "hél");
    assert_eq!(truncate_chars("hello", 0), "");
    assert_eq!(truncate_chars("hi", 10), "hi");
}

#[test]
fn expands_url_glob_range() {
    let expanded = expand_url_glob_seed("https://example.com/page/{1..3}");
    assert_eq!(
        expanded,
        vec![
            "https://example.com/page/1".to_string(),
            "https://example.com/page/2".to_string(),
            "https://example.com/page/3".to_string()
        ]
    );
}

#[test]
fn expands_url_glob_list_and_nested() {
    let expanded = expand_url_glob_seed("https://example.com/{news,docs}/{a,b}");
    assert_eq!(
        expanded,
        vec![
            "https://example.com/news/a".to_string(),
            "https://example.com/news/b".to_string(),
            "https://example.com/docs/a".to_string(),
            "https://example.com/docs/b".to_string()
        ]
    );
}

#[test]
fn expands_url_glob_with_total_cap() {
    let expanded = expand_url_glob_seed("https://example.com/page/{1..20000}");
    assert_eq!(expanded.len(), MAX_EXPANSION_TOTAL);
    assert_eq!(
        expanded.first().map(String::as_str),
        Some("https://example.com/page/1")
    );
    assert_eq!(
        expanded.last().map(String::as_str),
        Some("https://example.com/page/10000")
    );
}

#[test]
fn expands_url_glob_range_stops_on_overflow() {
    let expanded =
        expand_url_glob_seed("https://example.com/page/{9223372036854775806..9223372036854775807}");
    assert_eq!(
        expanded,
        vec![
            "https://example.com/page/9223372036854775806".to_string(),
            "https://example.com/page/9223372036854775807".to_string(),
        ]
    );
}

#[test]
fn start_url_from_cfg_guards_extract_job_subcommand_tokens() {
    let cfg = Config {
        command: CommandKind::Extract,
        start_url: "https://fallback.example".to_string(),
        positional: vec!["list".to_string()],
        ..Config::default()
    };

    assert_eq!(start_url_from_cfg(&cfg), "https://fallback.example");
}
