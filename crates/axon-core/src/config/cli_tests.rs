use super::Cli;
use clap::{Parser, error::ErrorKind};

#[test]
fn parse_mcp_transport_stdio_flag() {
    let result = Cli::try_parse_from(["axon", "mcp", "--transport", "stdio"]);
    assert!(
        result.is_ok(),
        "mcp --transport stdio should parse: {result:?}"
    );
}

#[test]
fn parse_mcp_transport_both_flag() {
    let result = Cli::try_parse_from(["axon", "mcp", "--transport", "both"]);
    assert!(
        result.is_ok(),
        "mcp --transport both should parse: {result:?}"
    );
}

#[test]
fn parse_migrate_flags() {
    let result = Cli::try_parse_from(["axon", "migrate", "--from", "cortex", "--to", "cortex_v2"]);
    assert!(
        result.is_ok(),
        "migrate --from --to should parse: {result:?}"
    );
}

#[test]
fn parse_retrieve_max_points_flag() {
    let result = Cli::try_parse_from([
        "axon",
        "retrieve",
        "https://example.com/docs",
        "--max-points",
        "25",
    ]);
    assert!(
        result.is_ok(),
        "retrieve --max-points should parse: {result:?}"
    );
}

#[test]
fn embed_accepts_watch_flag() {
    let result = Cli::try_parse_from(["axon", "embed", "/tmp/project", "--watch"]);
    assert!(result.is_ok(), "embed --watch should parse: {result:?}");
}

#[test]
fn code_search_watch_returns_tombstone_error() {
    let err = Cli::try_parse_from(["axon", "code-search-watch"]).unwrap_err();
    assert!(
        err.to_string().contains("use `axon embed <path> --watch`"),
        "{err}"
    );
}

#[test]
fn code_search_watch_rejects_extra_args_without_dispatch_panic() {
    let err = Cli::try_parse_from(["axon", "code-search-watch", "anything"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnknownArgument);
}

#[test]
fn parse_rejects_active_and_recent_together() {
    let result = Cli::try_parse_from(["axon", "--active", "--recent", "status"]);
    assert!(result.is_err(), "active/recent should conflict");
    assert_eq!(
        result.expect_err("conflict expected").kind(),
        ErrorKind::ArgumentConflict
    );
}

#[test]
fn parse_rejects_reclaimed_and_active_together() {
    let result = Cli::try_parse_from(["axon", "--reclaimed", "--active", "status"]);
    assert!(result.is_err(), "reclaimed/active should conflict");
    assert_eq!(
        result.expect_err("conflict expected").kind(),
        ErrorKind::ArgumentConflict
    );
}

#[test]
fn parse_rejects_invalid_search_time_range_value() {
    let result = Cli::try_parse_from(["axon", "--search-time-range", "decade", "search", "q"]);
    assert!(
        result.is_err(),
        "invalid search-time-range should fail clap parsing"
    );
    assert_eq!(
        result.expect_err("invalid value expected").kind(),
        ErrorKind::InvalidValue
    );
}
