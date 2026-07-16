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
fn retained_scrape_parses_scrape_specific_flags() {
    let result = Cli::try_parse_from([
        "axon",
        "scrape",
        "--inline",
        "--no-embed",
        "https://example.com",
    ]);
    assert!(
        result.is_ok(),
        "retained scrape flags should parse on the scrape surface: {result:?}"
    );
}

#[test]
fn map_rejects_removed_crawl_fallback_flag() {
    let err = Cli::try_parse_from([
        "axon",
        "map",
        "https://example.com",
        "--map-fallback",
        "crawl",
    ])
    .expect_err("map must not expose a crawl fallback");
    assert_eq!(err.kind(), ErrorKind::UnknownArgument);
}

/// Removed command tokens remain absent after the pipeline-unification clean
/// break (issue #298 P10). `scrape` is retained and must continue to parse.
#[test]
fn removed_commands_no_longer_parse() {
    for name in [
        "crawl",
        "embed",
        "ingest",
        "code-search",
        "code-search-watch",
        "purge",
        "dedupe",
    ] {
        let err = Cli::try_parse_from(["axon", name, "x"])
            .expect_err(&format!("`axon {name}` must not parse after removal"));
        assert_eq!(
            err.kind(),
            ErrorKind::InvalidSubcommand,
            "`axon {name}` should be an unknown subcommand, got: {err}"
        );
    }
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
