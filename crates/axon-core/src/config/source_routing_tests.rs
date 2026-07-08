use super::*;
use crate::config::build_cli_command;

fn route(args: &[&str]) -> Vec<String> {
    let command = build_cli_command();
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    route_bare_source(owned, &command)
}

#[test]
fn bare_web_url_routes_to_source() {
    assert_eq!(
        route(&["axon", "https://example.com"]),
        vec!["axon", "source", "https://example.com"]
    );
}

#[test]
fn bare_local_path_routes_to_source() {
    assert_eq!(route(&["axon", "./dir"]), vec!["axon", "source", "./dir"]);
}

#[test]
fn bare_subreddit_routes_to_source() {
    assert_eq!(route(&["axon", "r/rust"]), vec!["axon", "source", "r/rust"]);
}

#[test]
fn bare_registry_target_routes_to_source() {
    assert_eq!(
        route(&["axon", "pkg:npm/foo"]),
        vec!["axon", "source", "pkg:npm/foo"]
    );
}

/// Removed source-family command names (`embed`, `ingest`, `scrape`, `crawl`,
/// `code-search`, `code-search-watch`) are not registered clap subcommands
/// after the Phase 10 clean break, so they fall through the same bare-source
/// routing as any other unrecognized first token: `axon embed <path>` behaves
/// like `axon <path>` with `embed` treated as (the start of) the source value,
/// never dispatching to a removed command. There are no compatibility aliases.
#[test]
fn removed_command_names_route_to_source_not_dispatch() {
    for removed in [
        "embed",
        "ingest",
        "scrape",
        "crawl",
        "code-search",
        "code-search-watch",
    ] {
        assert_eq!(
            route(&["axon", removed, "https://example.com"]),
            vec!["axon", "source", removed, "https://example.com"],
            "removed command `{removed}` must route through `source`, not dispatch directly"
        );
    }
}

#[test]
fn explicit_source_subcommand_is_untouched() {
    assert_eq!(
        route(&["axon", "source", "https://example.com"]),
        vec!["axon", "source", "https://example.com"]
    );
}

#[test]
fn known_subcommand_is_untouched() {
    assert_eq!(
        route(&["axon", "ask", "what is rust"]),
        vec!["axon", "ask", "what is rust"]
    );
    assert_eq!(route(&["axon", "doctor"]), vec!["axon", "doctor"]);
    assert_eq!(route(&["axon", "serve"]), vec!["axon", "serve"]);
}

#[test]
fn subcommand_alias_is_untouched() {
    // `completion` is an alias of `completions`.
    assert_eq!(
        route(&["axon", "completion", "bash"]),
        vec!["axon", "completion", "bash"]
    );
}

#[test]
fn removed_purge_aliases_route_as_source() {
    // `delete-url` and `delete` were aliases of the removed `purge` command
    // (docs/pipeline-unification/delivery/surface-removal-contract.md). With
    // `purge` gone, these tokens are no longer known subcommands and route as
    // bare source arguments like any other unrecognized positional.
    assert_eq!(
        route(&["axon", "delete-url", "https://x"]),
        vec!["axon", "source", "delete-url", "https://x"]
    );
    assert_eq!(
        route(&["axon", "delete", "https://x"]),
        vec!["axon", "source", "delete", "https://x"]
    );
}

#[test]
fn help_and_version_are_untouched() {
    assert_eq!(route(&["axon", "--help"]), vec!["axon", "--help"]);
    assert_eq!(route(&["axon", "--version"]), vec!["axon", "--version"]);
    assert_eq!(route(&["axon", "help"]), vec!["axon", "help"]);
}

#[test]
fn program_name_only_is_untouched() {
    assert_eq!(route(&["axon"]), vec!["axon"]);
}

#[test]
fn leading_boolean_global_flag_before_bare_source() {
    // `--json` is a boolean flag (no value); the bare source token follows it.
    assert_eq!(
        route(&["axon", "--json", "r/rust"]),
        vec!["axon", "--json", "source", "r/rust"]
    );
}

#[test]
fn leading_value_global_flag_before_bare_source() {
    // `--collection foo` consumes `foo`; `r/rust` is the source position.
    assert_eq!(
        route(&["axon", "--collection", "foo", "r/rust"]),
        vec!["axon", "--collection", "foo", "source", "r/rust"]
    );
}

#[test]
fn leading_value_global_flag_before_known_subcommand() {
    // `--collection foo ask ...` — the value is consumed, `ask` is a known
    // subcommand, so nothing is injected.
    assert_eq!(
        route(&["axon", "--collection", "foo", "ask", "q"]),
        vec!["axon", "--collection", "foo", "ask", "q"]
    );
}

#[test]
fn equals_form_value_flag_before_bare_source() {
    // `--collection=foo` carries its own value; the next token is the source.
    assert_eq!(
        route(&["axon", "--collection=foo", "https://x"]),
        vec!["axon", "--collection=foo", "source", "https://x"]
    );
}

#[test]
fn scope_flag_after_bare_source_is_positional() {
    // The `--scope` belongs to the injected `source` subcommand; injection
    // happens at the first positional token (the URL), and clap later attaches
    // `--scope site` to `source`.
    assert_eq!(
        route(&["axon", "https://x", "--scope", "site"]),
        vec!["axon", "source", "https://x", "--scope", "site"]
    );
}
